use crate::StorageType;
use crate::error::StorageError;
use crate::{StorageEngine, VectorPage, checkpoint::StorageCheckpoint};
use defs::{DenseVector, Payload, PointId};
use std::path::{Path, PathBuf};

pub const INMEMORY_CHECKPOINT_FILENAME_MARKER: &str = "inmemory";

pub struct MemoryStorage {
    // define here how MemoryStorage will be defined
}

impl MemoryStorage {
    pub fn new() -> Self {
        MemoryStorage {}
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageEngine for MemoryStorage {
    fn insert_point(
        &self,
        _id: PointId,
        _vector: Option<DenseVector>,
        _payload: Option<Payload>,
    ) -> Result<(), StorageError> {
        Ok(())
    }
    fn contains_point(&self, _id: PointId) -> Result<bool, StorageError> {
        Ok(true)
    }
    fn delete_point(&self, _id: PointId) -> Result<(), StorageError> {
        Ok(())
    }
    fn get_payload(&self, _id: PointId) -> Result<Option<Payload>, StorageError> {
        Ok(None)
    }
    fn get_vector(&self, _id: PointId) -> Result<Option<DenseVector>, StorageError> {
        Ok(None)
    }
    fn list_vectors(
        &self,
        _offset: PointId,
        _limit: usize,
    ) -> Result<Option<VectorPage>, StorageError> {
        Ok(None)
    }

    fn checkpoint_at(&self, _path: &Path) -> Result<StorageCheckpoint, StorageError> {
        Ok(StorageCheckpoint {
            path: PathBuf::default(),
            storage_type: StorageType::InMemory,
        })
    }

    fn restore_checkpoint(&mut self, _checkpoint: &StorageCheckpoint) -> Result<(), StorageError> {
        Ok(())
    }
}
