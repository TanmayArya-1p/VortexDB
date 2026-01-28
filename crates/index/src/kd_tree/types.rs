use defs::{IndexedVector, OrdF32, PointId};
use std::cmp::Ordering;

// the node which will be the part of the KD Tree
pub struct KDTreeNode {
    pub indexed_vector: IndexedVector,
    pub left: Option<Box<KDTreeNode>>,
    pub right: Option<Box<KDTreeNode>>,
    pub is_deleted: bool,
    pub axis: usize,
    pub subtree_size: usize,
}

// The struct definition which is present in max heap while search
// distance is first for correct Ord derivation (primary sort key)
#[derive(Debug, Clone, PartialEq)]
pub struct Neighbor {
    pub distance: OrdF32,
    pub id: PointId,
}

impl Eq for Neighbor {}

// Custom Ord implementation for the max-heap
impl Ord for Neighbor {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance
            .partial_cmp(&other.distance)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for Neighbor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
