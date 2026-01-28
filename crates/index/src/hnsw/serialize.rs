use defs::DbError;
use storage::StorageEngine;

use crate::{IndexSnapshot, SerializableIndex, hnsw::HnswIndex};

impl SerializableIndex for HnswIndex {
    fn serialize_topology(&self) -> Result<Vec<u8>, DbError> {
        return Err(DbError::SerializationError("not implemented".to_string()));
    }
    fn serialize_metadata(&self) -> Result<Vec<u8>, DbError> {
        return Err(DbError::SerializationError("not implemented".to_string()));
    }

    fn snapshot(&self) -> Result<IndexSnapshot, DbError> {
        return Err(DbError::SerializationError("not implemented".to_string()));
    }

    fn populate_vectors(&mut self, _storage: &dyn StorageEngine) -> Result<(), DbError> {
        return Err(DbError::SerializationError("not implemented".to_string()));
    }
}
