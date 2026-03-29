use crate::{IndexError, VectorIndex, distance};
use defs::{DenseVector, DistanceOrderedVector, IndexedVector, PointId, Similarity};

pub struct FlatIndex {
    pub index: Vec<IndexedVector>,
}

impl FlatIndex {
    pub fn new() -> Self {
        Self { index: Vec::new() }
    }

    pub fn build(vectors: Vec<IndexedVector>) -> Self {
        FlatIndex { index: vectors }
    }
}

impl Default for FlatIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorIndex for FlatIndex {
    fn insert(&mut self, vector: IndexedVector) -> Result<(), IndexError> {
        self.index.push(vector);
        Ok(())
    }

    fn delete(&mut self, point_id: PointId) -> Result<bool, IndexError> {
        if let Some(pos) = self.index.iter().position(|vector| vector.id == point_id) {
            self.index.remove(pos);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn search(
        &self,
        query_vector: DenseVector,
        similarity: Similarity,
        k: usize,
    ) -> Result<Vec<PointId>, IndexError> {
        let scores = self
            .index
            .iter()
            .map(|point| DistanceOrderedVector {
                distance: distance(&point.vector, &query_vector, similarity),
                query_vector: &query_vector,
                point_id: Some(point.id),
            })
            .collect::<Vec<_>>();

        // select k smallest elements in scores using a max heap
        let mut heap = std::collections::BinaryHeap::<DistanceOrderedVector>::new();
        for score in scores {
            if heap.len() < k {
                heap.push(score);
            } else if score < *heap.peek().unwrap() {
                heap.pop();
                heap.push(score);
            }
        }
        Ok(heap
            .into_sorted_vec()
            .into_iter()
            .map(|v| v.point_id.unwrap())
            .collect())
    }
}
