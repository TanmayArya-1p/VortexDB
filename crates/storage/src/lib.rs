use crate::rocks_db::RocksDbStorage;
use defs::{DbError, DenseVector, Payload, PointId};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
pub mod checkpoint;

pub type VectorPage = (Vec<(PointId, DenseVector)>, PointId);

pub mod error;
pub mod in_memory;
pub mod rocks_db;

pub use error::{Result, StorageError};

pub trait StorageEngine: Send + Sync {
    fn insert_point(
        &self,
        id: PointId,
        vector: Option<DenseVector>,
        payload: Option<Payload>,
    ) -> Result<()>;
    fn get_vector(&self, id: PointId) -> Result<Option<DenseVector>>;
    fn get_payload(&self, id: PointId) -> Result<Option<Payload>>;
    fn delete_point(&self, id: PointId) -> Result<()>;
    fn contains_point(&self, id: PointId) -> Result<bool>;
    fn list_vectors(&self, offset: PointId, limit: usize) -> Result<Option<VectorPage>>;

    fn checkpoint_at(&self, path: &Path) -> Result<checkpoint::StorageCheckpoint, DbError>;
    fn restore_checkpoint(
        &mut self,
        checkpoint: &checkpoint::StorageCheckpoint,
    ) -> Result<(), DbError>;
}

pub mod in_memory;
pub mod rocks_db;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageType {
    InMemory,
    RocksDb,
}

pub fn create_storage_engine(
    storage_type: StorageType,
    path: impl Into<PathBuf>,
) -> Result<Arc<dyn StorageEngine>> {
    match storage_type {
        StorageType::InMemory => Ok(Arc::new(in_memory::MemoryStorage::new())),
        StorageType::RocksDb => match RocksDbStorage::new(path) {
            Ok(rocks_db) => Ok(Arc::new(rocks_db)),
            Err(e) => Err(e),
        },
    }
}
