use std::collections::HashSet;
use std::io::{Cursor, Read};

use super::index::KDTree;
use super::types::KDTreeNode;
use crate::SerializableIndexer;
use bincode;
use defs::{DbError, IndexedVector, PointId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const KD_TREE_MAGIC_BYTES: [u8; 4] = [0x00, 0x00, 0x00, 0x00];

#[derive(Serialize, Deserialize)]
pub struct KDTreeMetadata {
    pub dim: usize,
    pub total_nodes: usize,
    pub deleted_count: usize,
}

impl SerializableIndexer for KDTree {
    fn magic_bytes(&self) -> [u8; 4] {
        KD_TREE_MAGIC_BYTES
    }

    fn serialize_topology(&self) -> Result<Vec<u8>, DbError> {
        let mut buffer = Vec::new();
        self.serialize_topology_recursive(&self.root, &mut buffer)?;
        Ok(buffer)
    }

    fn serialize_metadata(&self) -> Result<Vec<u8>, DbError> {
        let mut buffer = Vec::new();
        let km = KDTreeMetadata {
            dim: self.dim,
            total_nodes: self.total_nodes,
            deleted_count: self.deleted_count,
        };
        let metadata_bytes = bincode::serialize(&km).map_err(|e| {
            DbError::SerializationError(format!("Failed to serailize KD Tree Metadata: {}", e))
        })?;
        buffer.extend_from_slice(metadata_bytes.as_slice());
        Ok(buffer)
    }
}

const NODE_MARKER_BYTE: u8 = 1u8;
const SKIP_MARKER_BYTE: u8 = 0u8;

const DELETED_MASK: u8 = 2u8;

impl KDTree {
    pub fn deserialize(
        metadata_bytes: Vec<u8>,
        topology_bytes: Vec<u8>,
    ) -> Result<Box<KDTree>, DbError> {
        let metadata: KDTreeMetadata =
            bincode::deserialize(metadata_bytes.as_slice()).map_err(|e| {
                DbError::SerializationError(format!(
                    "Failed to deserailize KD Tree Metadata: {}",
                    e
                ))
            })?;

        let mut buf = Cursor::new(topology_bytes);
        let mut non_deleted = HashSet::new();
        let root = deserialize_topology_recursive(&mut buf, &mut non_deleted)?;

        Ok(Box::new(KDTree {
            dim: metadata.dim,
            root,
            point_ids: non_deleted,
            total_nodes: metadata.total_nodes,
            deleted_count: metadata.deleted_count,
        }))
    }

    fn serialize_topology_recursive(
        &self,
        current_opt: &Option<Box<KDTreeNode>>,
        buffer: &mut Vec<u8>,
    ) -> Result<(), DbError> {
        if let Some(current) = current_opt {
            let mut marker = NODE_MARKER_BYTE;
            if current.is_deleted {
                marker |= DELETED_MASK;
            }
            buffer.push(marker);

            let uuid_bytes = current.indexed_vector.id.to_bytes_le();
            buffer.extend_from_slice(&uuid_bytes);

            // serialize left subtree topology
            self.serialize_topology_recursive(&current.left, buffer)?;
            // serialize right subtree topology
            self.serialize_topology_recursive(&current.right, buffer)?;
        } else {
            buffer.push(SKIP_MARKER_BYTE);
        }
        Ok(())
    }
}

fn deserialize_topology_recursive(
    buffer: &mut Cursor<Vec<u8>>,
    non_deleted: &mut HashSet<PointId>,
) -> Result<Option<Box<KDTreeNode>>, DbError> {
    let mut current_marker: [u8; 1] = [0u8; 1];
    buffer.read_exact(&mut current_marker).map_err(|e| {
        DbError::SerializationError(format!("Failed to deserialize KD Topology: {}", e))
    })?;

    if current_marker[0] == SKIP_MARKER_BYTE {
        return Ok(None);
    }

    let mut uuid_bytes = [0u8; 16];
    buffer.read_exact(&mut uuid_bytes).map_err(|e| {
        DbError::SerializationError(format!("Failed to deserialize KD Topology: {}", e))
    })?;
    let uuid = Uuid::from_bytes_le(uuid_bytes);
    let indexed_vector = IndexedVector {
        id: uuid,
        vector: Vec::new(),
    };

    let is_deleted = current_marker[0] & DELETED_MASK == DELETED_MASK;
    if !is_deleted {
        non_deleted.insert(uuid);
    }

    // pre order deserialization
    let left_node = deserialize_topology_recursive(buffer, non_deleted)?;
    let right_node = deserialize_topology_recursive(buffer, non_deleted)?;

    let left_size = left_node.as_ref().map_or(0, |n| n.subtree_size);
    let right_size = right_node.as_ref().map_or(0, |n| n.subtree_size);

    let current_node = KDTreeNode {
        indexed_vector,
        left: left_node,
        right: right_node,
        is_deleted,
        subtree_size: left_size + right_size + 1,
    };

    Ok(Some(Box::new(current_node)))
}
