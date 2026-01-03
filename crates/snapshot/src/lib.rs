pub mod constants;
pub mod manifest;
pub mod metadata;
mod util;

use crate::{
    constants::{MANIFEST_FILE, SNAPSHOT_PARSER_VER, STORAGE_CHECKPOINT_FILE},
    manifest::Manifest,
    util::{compress_archive, save_index_metadata, save_topology},
};

use chrono::{DateTime, Local};
use defs::DbError;
use flate2::read::GzDecoder;
use index::{IndexSnapshot, VectorIndex};
use semver::Version;
use std::{
    fs::File, path::{Path}, time::SystemTime
};
use storage::{StorageCheckpoint, StorageEngine, rocks_db::RocksDbStorage};
use tar::Archive;
use tempfile::tempdir;
use uuid::Uuid;

// TODO: implement snapshot engine that runs in its own thread and wakes up in regular intervals

pub struct Snapshot {
    pub id: Uuid,
    pub date: SystemTime,
    pub sem_ver: Version,
    pub index_snapshot: IndexSnapshot,
    pub storage_snapshot: StorageCheckpoint,
}

impl Snapshot {
    pub fn new(index_snapshot : IndexSnapshot, storage_snapshot : StorageCheckpoint) -> Self {
        let id = Uuid::new_v4();
        let date = SystemTime::now();

        Snapshot {
            id,
            date,
            sem_ver: SNAPSHOT_PARSER_VER,
            index_snapshot,
            storage_snapshot,
        }
    }

    pub fn save(
        &self,
        path: &Path
    ) -> Result<(), DbError> {

        if !path.is_dir() {
            return Err(DbError::SnapshotError(format!(
                "Invalid path: {}",
                path.display()
            )));
        }

        let temp_dir = tempdir().map_err(|e| DbError::SnapshotError(e.to_string()))?;

        // save index snapshots
        let index_metadata_path =
            save_index_metadata(temp_dir.path(), self.id, &self.index_snapshot.metadata_b, &self.index_snapshot.magic, dimensions)?;

        let topology_path = save_topology(temp_dir.path(), self.id, &self.index_snapshot.topology_b, &self.index_snapshot.magic)?;

        // take checksums
        let index_metadata_checksum = util::sha256_digest(&index_metadata_path)
            .map_err(|e| DbError::SnapshotError(e.to_string()))?;
        let index_topo_checksum = util::sha256_digest(&topology_path)
            .map_err(|e| DbError::SnapshotError(e.to_string()))?;
        let storage_checkpoint_checksum = util::sha256_digest(&self.storage_snapshot.path)
            .map_err(|e| DbError::SnapshotError(e.to_string()))?;

        let dt_now_local: DateTime<Local> = self.date.into();

        // create manifest file
        let manifest = Manifest {
            id: self.id,
            date: dt_now_local.timestamp(),
            sem_ver: constants::SNAPSHOT_PARSER_VER.to_string(),
            index_metadata_checksum,
            index_topo_checksum,
            storage_checkpoint_checksum,
        };

        let manifest_path = manifest
            .save(temp_dir.path())
            .map_err(|e| DbError::SnapshotError(e.to_string()))?;


        let tar_filename = format!(
            "{}.tar.gz",
            metadata::Metadata::new(
                self.id,
                self.date,
                index_metadata_path.clone(),
                constants::SNAPSHOT_PARSER_VER
            )
        );
        let tar_gz_path = path.join(tar_filename);

        compress_archive(
            &tar_gz_path,
            &[
                &index_metadata_path,
                &topology_path,
                &self.storage_snapshot.path,
                &manifest_path,
            ],
            temp_dir.path(),
        )
        .map_err(|e| DbError::SnapshotError(e.to_string()))?;
        Ok(())
    }

    pub fn load(
        path: &Path,
        storage_data_path : &Path
    ) -> Result<(Box<dyn VectorIndex>, Box<dyn StorageEngine>, usize), DbError> {

        // only rocksdb is supported for snapshots as of now
        let mut storage_engine = Box::new(RocksDbStorage::new(storage_data_path)).map_err(|e| DbError::SnapshotError(format!("Failed to reinitialize storage engine: {}",e)))?;

        let tar_gz = File::open(path)
            .map_err(|e| DbError::SnapshotError(format!("Couldn't open snapshot: {}", e)))?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);

        let snapshot_filename = path.file_name().ok_or(DbError::SnapshotError(
            "Invalid snapshot filename".to_string(),
        ))?;
        let temp_dir = std::env::temp_dir().join(snapshot_filename);

        // remove any existing data
        if temp_dir.exists() && !temp_dir.is_dir() {
            std::fs::remove_file(temp_dir.clone()).map_err(|e| {
                DbError::SnapshotError(format!("Couldn't remove existing file: {}", e))
            })?;
        } else if temp_dir.is_dir() {
            std::fs::remove_dir_all(temp_dir.clone()).map_err(|e| {
                DbError::SnapshotError(format!("Couldn't remove existing directory: {}", e))
            })?;
        }

        std::fs::create_dir(temp_dir.clone()).map_err(|e| {
            DbError::SnapshotError(format!("Couldn't create temporary directory: {}", e))
        })?;

        archive
            .unpack(temp_dir.clone())
            .map_err(|e| DbError::SnapshotError(format!("Couldn't unpack archive: {}", e)))?;

        // read manifest and validate
        let manifest_path = temp_dir.join(MANIFEST_FILE);
        if !manifest_path.is_file() {
            return Err(DbError::SnapshotError(
                "Manifest file not found".to_string(),
            ));
        }

        let manifest = Manifest::load(&manifest_path)
            .map_err(|e| DbError::SnapshotError(format!("Couldn't load manifest: {}", e)))?;

        if manifest.sem_ver != SNAPSHOT_PARSER_VER.to_string() {
            return Err(DbError::SnapshotError(
                "Incompatible snapshot version".to_string(),
            ));
        }

        let id = manifest.id;
        let index_metadata_path = temp_dir.join(util::metadata_file_name(&id));
        let topology_path = temp_dir.join(util::topology_file_name(&id));
        let storage_checkpoint_path = temp_dir.join(STORAGE_CHECKPOINT_FILE);

        if !index_metadata_path.exists()
            || !topology_path.exists()
            || !storage_checkpoint_path.exists()
        {
            return Err(DbError::SnapshotError("Missing snapshot files".to_string()));
        }

        // match checksums
        if util::sha256_digest(&index_metadata_path).map_err(|_| {
            DbError::SnapshotError("Could not calculate index metadata hash".to_string())
        })? != manifest.index_metadata_checksum
        {
            return Err(DbError::SnapshotError(
                "Index metadata hash mismatch".to_string(),
            ));
        }
        if util::sha256_digest(&topology_path)
            .map_err(|_| DbError::SnapshotError("Could not calculate topology hash".to_string()))?
            != manifest.index_topo_checksum
        {
            return Err(DbError::SnapshotError("Topology hash mismatch".to_string()));
        }
        if util::sha256_digest(&storage_checkpoint_path).map_err(|_| {
            DbError::SnapshotError("Could not calculate storage checkpoint hash".to_string())
        })? != manifest.storage_checkpoint_checksum
        {
            return Err(DbError::SnapshotError(
                "Storage checkpoint hash mismatch".to_string(),
            ));
        }

        let (mgmeta, dimensions, meta_bytes) = util::read_index_metadata(&index_metadata_path)
            .map_err(|_| DbError::SnapshotError("Could not read metadata".to_string()))?;
        let (mgtopo, topo_bytes) = util::read_index_topology(&topology_path)
            .map_err(|_| DbError::SnapshotError("Could not read topology".to_string()))?;

        if mgtopo != mgmeta {
            return Err(DbError::InvalidMagicBytes(
                "Magic bytes don't match".to_string(),
            ));
        }

        storage_engine.restore_checkpoint(&storage_checkpoint_path)?;
        let storage_engine_boxed: Box<dyn StorageEngine> = Box::new(storage_engine);

        let vector_index : Box<dyn VectorIndex> = index::deserialize(
            meta_bytes,
            topo_bytes,
            index::index_type_from_magic(mgmeta)?,
        )?;

        Ok((vector_index, storage_engine_boxed, dimensions))
    }
}
