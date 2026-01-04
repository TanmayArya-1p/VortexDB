use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::registry::{INFINITY_LIMIT, NO_OFFSET, SnapshotRegistry};
use crate::registry::{SnapshotMetaPage, constants::LOCAL_REGISTRY_LOCKFILE};
use crate::{
    Snapshot, VectorDbRestore,
    metadata::{Metadata, SmallID},
};
use defs::DbError;
use fs2::FileExt;

pub struct LocalRegistry {
    pub dir: PathBuf,
    filename_cache: HashMap<SmallID, String>,
}

impl LocalRegistry {
    pub fn new(dir: &Path) -> Result<LocalRegistry, DbError> {
        fs::create_dir_all(dir).map_err(|e| DbError::SnapshotRegistryError(e.to_string()))?;
        let lock_file_path = dir.join(LOCAL_REGISTRY_LOCKFILE);
        let lock_file = if !lock_file_path.exists() {
            fs::File::create(&lock_file_path).map_err(|e| {
                DbError::SnapshotRegistryError(format!("Couldn't create LOCKFILE : {}", e))
            })?
        } else {
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&lock_file_path)
                .map_err(|e| {
                    DbError::SnapshotRegistryError(format!("Couldn't open LOCKFILE : {}", e))
                })?
        };

        // try to acquire lockfile
        lock_file
            .try_lock_exclusive()
            .map_err(|_| DbError::SnapshotRegistryError("Couldn't acquire LOCKFILE".to_string()))?;

        Ok(LocalRegistry {
            dir: dir.to_path_buf(),
            filename_cache: HashMap::new(),
        })
    }
}

impl SnapshotRegistry for LocalRegistry {
    fn add_snapshot(&mut self, snapshot: &Snapshot) -> Result<(), DbError> {
        snapshot.save(self.dir.as_path())?;
        Ok(())
    }

    fn list_snapshots(&mut self, limit: usize, offset: usize) -> Result<SnapshotMetaPage, DbError> {
        let mut res = Vec::new();
        let filtered_files = fs::read_dir(self.dir.as_path())
            .map_err(|e| {
                DbError::SnapshotRegistryError(format!("Cannot read local registry dir: {}", e))
            })?
            .skip(offset)
            .take(limit);

        for file in filtered_files {
            let file = match file {
                Ok(file) => file,
                Err(_) => continue,
            };
            let file_path = file.path();
            let metadata = Metadata::parse(file_path.as_path())?;

            let filename = file_path
                .file_name()
                .ok_or(DbError::SnapshotRegistryError(
                    "Could not load filename of snapshot".to_string(),
                ))?
                .to_string_lossy();
            self.filename_cache
                .insert(metadata.small_id.clone(), filename.to_string());

            res.push(metadata);
        }
        Ok(res)
    }

    fn list_alive_snapshots(&mut self) -> Result<SnapshotMetaPage, DbError> {
        self.list_snapshots(INFINITY_LIMIT, NO_OFFSET)
    }

    fn remove_snapshot(&mut self, small_id: SmallID) -> Result<Metadata, DbError> {
        if let Some(filename) = self.filename_cache.get(&small_id) {
            let snapshot_filepath = self.dir.join(filename);

            let metadata = Metadata::parse(snapshot_filepath.as_path())?;
            fs::remove_file(snapshot_filepath.as_path()).map_err(|e| {
                DbError::SnapshotRegistryError(format!("Failed to remove snapshot: {}", e))
            })?;
            self.filename_cache.remove_entry(&small_id);
            Ok(metadata)
        } else {
            for file in fs::read_dir(self.dir.as_path()).map_err(|e| {
                DbError::SnapshotRegistryError(format!("Cannot read local registry dir: {}", e))
            })? {
                let file = match file {
                    Ok(file) => file,
                    Err(_) => continue,
                };
                let file_path = file.path();
                let metadata = Metadata::parse(file_path.as_path())?;

                if metadata.small_id == small_id {
                    return Ok(metadata);
                }
            }
            Err(DbError::SnapshotRegistryError(
                "Snapshot not found".to_string(),
            ))
        }
    }

    fn mark_dead(&mut self, small_id: String) -> Result<Metadata, DbError> {
        self.remove_snapshot(small_id)
    }

    fn load(
        &mut self,
        small_id: String,
        storage_data_path: &Path,
    ) -> Result<VectorDbRestore, DbError> {
        if let Some(filename) = self.filename_cache.get(&small_id) {
            let snapshot_filepath = self.dir.join(filename);
            Snapshot::load(snapshot_filepath.as_path(), storage_data_path)
        } else {
            for file in fs::read_dir(self.dir.as_path()).map_err(|e| {
                DbError::SnapshotRegistryError(format!("Cannot read local registry dir: {}", e))
            })? {
                let file = match file {
                    Ok(file) => file,
                    Err(_) => continue,
                };
                let file_path = file.path();
                let metadata = Metadata::parse(file_path.as_path())?;
                let filename = file_path
                    .file_name()
                    .ok_or(DbError::SnapshotRegistryError(
                        "Could not load filename of snapshot".to_string(),
                    ))?
                    .to_string_lossy();
                self.filename_cache
                    .insert(metadata.small_id.clone(), filename.to_string());
                if metadata.small_id == small_id {
                    return Snapshot::load(file_path.as_path(), storage_data_path);
                }
            }
            Err(DbError::SnapshotRegistryError(
                "Snapshot not found".to_string(),
            ))
        }
    }

    fn dir(&self) -> PathBuf {
        self.dir.clone()
    }
}

impl Drop for LocalRegistry {
    fn drop(&mut self) {
        // remove exclusive lock on lockfile
        let lock_file_path = self.dir.join(LOCAL_REGISTRY_LOCKFILE);
        if let Ok(lock_file) = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_file_path)
        {
            let _ = lock_file.unlock();
        }
    }
}
