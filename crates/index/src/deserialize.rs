use defs::{DbError};

use crate::flat::index::FlatIndex;
use crate::kd_tree::index::KDTree;
use crate::{IndexSnapshot, IndexType, VectorIndex};

pub fn deserialize(
    snapshot: &IndexSnapshot
) -> Result<Box<dyn VectorIndex>, DbError> {
    match snapshot.index_type {
        IndexType::Flat => Ok(Box::new(FlatIndex::deserialize(snapshot)?)),
        IndexType::KDTree => Ok(Box::new(KDTree::deserialize(snapshot)?)),
        IndexType::HNSW => Ok(Box::new(FlatIndex::deserialize(snapshot)?)), // TODO: change this for hnsw
    }
}
