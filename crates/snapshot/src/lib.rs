pub mod types;
mod util;

use crate::types::{Snapshot, SnapshotManifest};
use chrono::{DateTime, Local};
use defs::DbError;
use index::VectorIndex;
use semver::Version;
use std::{path::PathBuf, time::SystemTime};
use storage::StorageEngine;
use tempfile::tempdir;
use uuid::Uuid;

const SNAPSHOT_PARSER_VER: Version = Version::new(0, 1, 0);

// TODO: implement snapshot engine that runs in its own thread and wakes up in regular intervals

impl Snapshot {
    pub fn create(
        index: &dyn VectorIndex,
        storage: &dyn StorageEngine,
        path: PathBuf,
    ) -> Result<Self, DbError> {
        let id = Uuid::new_v4();
        let date = SystemTime::now();

        if !path.is_dir() {
            return Err(DbError::SnapshotError(format!(
                "Invalid path: {}",
                path.display()
            )));
        }

        let temp_dir = tempdir().map_err(|e| DbError::SnapshotError(e.to_string()))?;

        let index_metadata_b = index.serialize_metadata()?;
        let index_topology_b = index.serialize_topology()?;

        let magic_b = index.magic_bytes();

        // save index snapshots
        let metadata_path = Self::save_metadata(temp_dir.path(), id, &index_metadata_b, &magic_b)?;
        let topology_path = Self::save_topology(temp_dir.path(), id, &index_topology_b, &magic_b)?;

        // save storage checkpoint
        let storage_checkpoint_path = temp_dir.path().join("storage-checkpoint.tar.gz");
        storage.checkpoint(&storage_checkpoint_path)?;

        // take checksums
        let index_metadata_checksum = util::sha256_digest(&metadata_path)
            .map_err(|e| DbError::SnapshotError(e.to_string()))?;
        let index_topo_checksum = util::sha256_digest(&topology_path)
            .map_err(|e| DbError::SnapshotError(e.to_string()))?;
        let storage_checkpoint_checksum = util::sha256_digest(&storage_checkpoint_path)
            .map_err(|e| DbError::SnapshotError(e.to_string()))?;

        let dt_now_local: DateTime<Local> = date.into();

        // create manifest file
        let manifest = SnapshotManifest {
            id,
            date: dt_now_local.timestamp(),
            sem_ver: SNAPSHOT_PARSER_VER.to_string(),
            index_metadata_checksum,
            index_topo_checksum,
            storage_checkpoint_checksum,
        };

        let manifest_path = Self::save_manifest(temp_dir.path(), &manifest)
            .map_err(|e| DbError::SnapshotError(e.to_string()))?;

        let tar_filename = format!(
            "{}-{}-{}.tar.gz",
            dt_now_local.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            &(id.to_string()[..5]),
            SNAPSHOT_PARSER_VER
        );
        let tar_gz_path = path.join(tar_filename);

        Self::compress_archive(
            &tar_gz_path,
            &[
                &metadata_path,
                &topology_path,
                &storage_checkpoint_path,
                &manifest_path,
            ],
            temp_dir.path(),
        )
        .map_err(|e| DbError::SnapshotError(e.to_string()))?;

        Ok(Snapshot {
            id,
            date,
            path: tar_gz_path,
            sem_ver: SNAPSHOT_PARSER_VER,
        })
    }

    // fn open(path : PathBuf) -> Result<Self, DbError>;

    // fn load(&self) -> Result((Box<dyn StorageEngine>, Box<dyn VectorIndex>))
}
