use defs::{DbError, DenseVector, Payload, PointId};
use tempfile::TempDir;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::rocks_db::{ROCKSDB_CHECKPOINT_FILENAME_MARKER, RocksDbStorage};

pub type VectorPage = (Vec<(PointId, DenseVector)>, PointId);

pub trait StorageEngine: Send + Sync {
    fn insert_point(
        &self,
        id: PointId,
        vector: Option<DenseVector>,
        payload: Option<Payload>,
    ) -> Result<(), DbError>;
    fn get_vector(&self, id: PointId) -> Result<Option<DenseVector>, DbError>;
    fn get_payload(&self, id: PointId) -> Result<Option<Payload>, DbError>;
    fn delete_point(&self, id: PointId) -> Result<(), DbError>;
    fn contains_point(&self, id: PointId) -> Result<bool, DbError>;
    fn list_vectors(&self, offset: PointId, limit: usize) -> Result<Option<VectorPage>, DbError>;

    fn checkpoint_at(&self, path: &Path) -> Result<StorageCheckpoint, DbError>;
    fn restore_checkpoint(&mut self, checkpoint: &StorageCheckpoint) -> Result<(), DbError>;
}

pub mod in_memory;
pub mod rocks_db;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum StorageType {
    InMemory,
    RocksDb,
}

pub fn create_storage_engine(
    storage_type: StorageType,
    path: impl Into<PathBuf>,
) -> Result<Arc<dyn StorageEngine>, DbError> {
    match storage_type {
        StorageType::InMemory => Ok(Arc::new(in_memory::MemoryStorage::new())),
        StorageType::RocksDb => match RocksDbStorage::new(path) {
            Ok(rocks_db) => Ok(Arc::new(rocks_db)),
            Err(e) => Err(e),
        },
    }
}


pub struct StorageCheckpoint {
    pub path: PathBuf,
    pub storage_type: StorageType,
}

impl StorageCheckpoint {
    fn open(path: &Path) -> Result<StorageCheckpoint, DbError> {
        let filename = path.file_name().ok_or_else(|| DbError::StorageCheckpointError("Invalid filename".to_string()))?.to_str().ok_or_else(|| DbError::StorageCheckpointError("Invalid UTF-8 in filename".to_string()))?.to_owned();
        let marker = filename.split_once("-").ok_or_else(|| DbError::StorageCheckpointError("Invalid filename".to_string()))?.0;

        let storage_type = match marker {
            ROCKSDB_CHECKPOINT_FILENAME_MARKER => StorageType::RocksDb,
            _ => return Err(DbError::StorageCheckpointError("Invalid storage type".to_string())),
        };

        Ok(StorageCheckpoint {
            path: path.to_path_buf(),
            storage_type,
        })
    }
}
