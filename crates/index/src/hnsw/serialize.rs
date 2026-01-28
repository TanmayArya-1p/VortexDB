use std::collections::HashMap;

use defs::{DbError, Dimension, PointId, Similarity};
use serde::{Deserialize, Serialize};
use storage::StorageEngine;

use crate::{
    IndexSnapshot, IndexType, SerializableIndex,
    hnsw::{
        HNSW_MAGIC_BYTES, HnswIndex,
        types::{LevelGenerator, Node, PointIndexation},
    },
};

#[repr(packed)]
#[derive(Serialize, Deserialize)]
pub struct HnswMetadataPack {
    pub ef_construction: usize,
    pub data_dimension: Dimension,
    pub ef: usize,
    pub similarity: Similarity,
}

#[derive(Serialize, Deserialize)]
pub struct HnswIndexPack {
    pub max_connections: usize,
    pub max_connections_0: usize,
    pub max_layer: usize,
    pub points_by_layer: Vec<Vec<PointId>>,
    pub nodes: Vec<Node>,
    pub entry_point: Option<PointId>,
    pub level_scale: f64,
}

impl SerializableIndex for HnswIndex {
    fn serialize_topology(&self) -> Result<Vec<u8>, DbError> {
        let mut buffer = Vec::new();

        let nodes: Vec<Node> = self.index.nodes.values().cloned().collect();
        let index_pack = HnswIndexPack {
            max_connections: self.index.max_connections,
            max_connections_0: self.index.max_connections_0,
            max_layer: self.index.max_layer,
            points_by_layer: self.index.points_by_layer.clone(),
            nodes,
            entry_point: self.index.entry_point,
            level_scale: self.index.level_generator.level_scale,
        };

        let index_bytes = bincode::serialize(&index_pack)
            .map_err(|e| DbError::SerializationError(e.to_string()))?;
        buffer.extend(index_bytes);

        return Ok(buffer);
    }

    fn serialize_metadata(&self) -> Result<Vec<u8>, DbError> {
        let mut buffer = Vec::new();
        let index_pack = HnswMetadataPack {
            ef_construction: self.ef_construction,
            data_dimension: self.data_dimension,
            ef: self.ef,
            similarity: self.similarity,
        };

        let metadata_bytes = bincode::serialize(&index_pack)
            .map_err(|e| DbError::SerializationError(e.to_string()))?;
        buffer.extend(metadata_bytes);
        return Ok(buffer);
    }

    fn snapshot(&self) -> Result<IndexSnapshot, DbError> {
        let topology_bytes = self.serialize_topology()?;
        let metadata_bytes = self.serialize_metadata()?;
        Ok(IndexSnapshot {
            index_type: crate::IndexType::HNSW,
            magic: HNSW_MAGIC_BYTES,
            topology_b: topology_bytes,
            metadata_b: metadata_bytes,
        })
    }

    fn populate_vectors(&mut self, storage: &dyn StorageEngine) -> Result<(), DbError> {
        // assumes index topology is restored
        for id in self.index.nodes.keys() {
            let vec = storage
                .get_vector(*id)?
                .ok_or(DbError::SerializationError(format!(
                    "Failed to locate vector for id: {} in storage",
                    id
                )))?;
            self.cache.insert(*id, vec);
        }
        Ok(())
    }
}

impl HnswIndex {
    pub fn deserialize(
        IndexSnapshot {
            index_type,
            magic,
            topology_b,
            metadata_b,
        }: &IndexSnapshot,
    ) -> Result<HnswIndex, DbError> {
        if index_type != &IndexType::HNSW {
            return Err(DbError::SerializationError(
                "Invalid index type".to_string(),
            ));
        }

        if magic != &HNSW_MAGIC_BYTES {
            return Err(DbError::SerializationError(
                "Invalid magic bytes".to_string(),
            ));
        }

        let metadata: HnswMetadataPack = bincode::deserialize(metadata_b).map_err(|e| {
            DbError::SerializationError(format!("Failed to deserialize HNSW Metadata: {}", e))
        })?;

        let index_pack: HnswIndexPack = bincode::deserialize(topology_b).map_err(|e| {
            DbError::SerializationError(format!("Failed to deserialize HNSW Index: {}", e))
        })?;

        let mut hnsw_index_restored = PointIndexation {
            max_connections: index_pack.max_connections,
            max_connections_0: index_pack.max_connections_0,
            max_layer: index_pack.max_layer,
            points_by_layer: index_pack.points_by_layer,
            entry_point: index_pack.entry_point,
            nodes: HashMap::new(),
            level_generator: LevelGenerator {
                level_scale: index_pack.level_scale,
            },
        };

        // restore nodes hashmap
        for i in index_pack.nodes {
            hnsw_index_restored.nodes.insert(i.id, i);
        }

        let hnsw = HnswIndex {
            ef_construction: metadata.ef_construction,
            data_dimension: metadata.data_dimension,
            ef: metadata.ef,
            cache: HashMap::new(),
            similarity: metadata.similarity,
            index: hnsw_index_restored,
        };

        Ok(hnsw)
    }
}
