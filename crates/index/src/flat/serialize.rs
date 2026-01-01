use crate::SerializableIndexer;
use crate::flat::index::FlatIndex;
use defs::{DbError, IndexedVector};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};
use uuid::Uuid;

const FLAT_MAGIC_BYTES: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatIndexMetadata {
    total_points: usize,
}

impl FlatIndex {
    pub fn deserialize(
        metadata_bytes: Vec<u8>,
        topology_bytes: Vec<u8>,
    ) -> Result<Box<FlatIndex>, DbError> {
        let metadata: FlatIndexMetadata = bincode::deserialize(&metadata_bytes).map_err(|e| {
            DbError::SerializationError(format!("Failed to deserialize FlatIndex Metadata: {}", e))
        })?;
        let total_points = metadata.total_points;

        let mut cursor = Cursor::new(topology_bytes);
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

        Ok(Box::new(FlatIndex { index: vectors }))
    }
}

impl SerializableIndexer for FlatIndex {
    fn magic_bytes(&self) -> [u8; 4] {
        FLAT_MAGIC_BYTES
    }

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
}
