use super::FLAT_MAGIC_BYTES;
use crate::{IndexSnapshot, SerializableIndex};
use crate::flat::index::FlatIndex;
use defs::{DbError, IndexedVector};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};
use uuid::Uuid;
use crate::IndexType;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatIndexMetadata {
    total_points: usize,
}

impl FlatIndex {
    pub fn deserialize(
        IndexSnapshot { index_type, magic, topology_b, metadata_b }: &IndexSnapshot
    ) -> Result<FlatIndex, DbError> {

        if magic != &FLAT_MAGIC_BYTES {
            return Err(DbError::SerializationError(format!("Invalid magic bytes")));
        }

        let metadata: FlatIndexMetadata = bincode::deserialize(metadata_b).map_err(|e| {
            DbError::SerializationError(format!("Failed to deserialize FlatIndex Metadata: {}", e))
        })?;
        let total_points = metadata.total_points;

        let mut cursor = Cursor::new(topology_b);
        let mut vectors = Vec::new();

        for _ in 0..total_points {
            let mut uuid_slice = [0u8; 16];
            cursor.read_exact(&mut uuid_slice).map_err(|e| {
                DbError::SerializationError(format!(
                    "Failed to deserialize FlatIndex Topology: {}",
                    e
                ))
            })?;
            let id = Uuid::from_bytes_le(uuid_slice);
            vectors.push(IndexedVector {
                id,
                vector: Vec::new(),
            });
        }

        Ok(FlatIndex { index: vectors })
    }
}

impl SerializableIndex for FlatIndex {

    fn serialize_topology(&self) -> Result<Vec<u8>, DbError> {
        let mut buffer: Vec<u8> = Vec::new();
        for point in &self.index {
            buffer.extend_from_slice(&point.id.to_bytes_le());
        }
        Ok(buffer)
    }

    fn serialize_metadata(&self) -> Result<Vec<u8>, DbError> {
        let mut buffer: Vec<u8> = Vec::new();
        let metadata = FlatIndexMetadata {
            total_points: self.index.len(),
        };

        let metadata_bytes = bincode::serialize(&metadata).map_err(|e| {
            DbError::SerializationError(format!("Failed to serialize FlatIndex Metadata: {}", e))
        })?;
        buffer.extend_from_slice(&metadata_bytes);
        Ok(buffer)
    }

    fn populate_vectors(&mut self, storage: &dyn storage::StorageEngine) -> Result<(), DbError> {
        for item in &mut self.index {
            item.vector = storage.get_vector(item.id)?.ok_or(DbError::VectorNotFound(item.id))?;
        }
        Ok(())
    }

    fn snapshot(&self) -> Result<IndexSnapshot, DbError> {
        let topology = self.serialize_topology()?;
        let metadata = self.serialize_metadata()?;

        Ok(IndexSnapshot {
            metadata_b: metadata,
            topology_b: topology,
            magic: FLAT_MAGIC_BYTES,
            index_type: IndexType::Flat,
        })
    }
}
