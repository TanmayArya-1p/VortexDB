use defs::{DbError, DenseVector, IndexedVector, Magic, PointId, Similarity};
use storage::StorageEngine;

pub mod flat;
pub mod kd_tree;

mod deserialize;
pub use crate::deserialize::*;


pub trait VectorIndex: Send + Sync + SerializableIndex {
    fn insert(&mut self, vector: IndexedVector) -> Result<(), DbError>;

    // Returns true if point id existed and is deleted, else returns false
    fn delete(&mut self, point_id: PointId) -> Result<bool, DbError>;

    fn search(
        &self,
        query_vector: DenseVector,
        similarity: Similarity,
        k: usize,
    ) -> Result<Vec<PointId>, DbError>; // Return a Vec of ids of closest vectors (length max k)

    // fn build() -> Result<(), DbError>; move this to impl for dyn compatibility
}

/// Distance function to get the distance between two vectors (taken from old version)
pub fn distance(a: &DenseVector, b: &DenseVector, dist_type: Similarity) -> f32 {
    assert_eq!(a.len(), b.len());
    match dist_type {
        Similarity::Euclidean => {
            let score: Vec<f32> = a
                .iter()
                .zip(b.iter())
                .map(|(&x, &y)| (x - y) * (x - y))
                .collect();
            score.iter().sum::<f32>().sqrt()
        }
        Similarity::Manhattan => {
            let score: Vec<f32> = a
                .iter()
                .zip(b.iter())
                .map(|(&x, &y)| (x - y).abs())
                .collect();
            score.iter().sum::<f32>()
        }
        Similarity::Hamming => {
            let score: Vec<f32> = a
                .iter()
                .zip(b.iter())
                .map(|(&x, &y)| if (x - y).abs() > 1e-8 { 1f32 } else { 0f32 })
                .collect();
            score.iter().sum::<f32>()
        }
        Similarity::Cosine => {
            let p_score: Vec<f32> = a.iter().zip(b.iter()).map(|(&x, &y)| x * y).collect();
            let p = p_score.iter().sum::<f32>();
            let q_score: Vec<f32> = a.iter().map(|&n| n * n).collect();
            let q = q_score.iter().sum::<f32>().sqrt();
            let r_score: Vec<f32> = b.iter().map(|&n| n * n).collect();
            let r = r_score.iter().sum::<f32>().sqrt();
            1.0 - (p / (q * r))
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IndexType {
    Flat,
    KDTree,
    HNSW,
}

pub struct IndexSnapshot {
    pub index_type: IndexType,
    pub magic: Magic,
    pub topology_b: Vec<u8>,
    pub metadata_b: Vec<u8>,
}

pub trait SerializableIndex {
    fn serialize_topology(&self) -> Result<Vec<u8>, DbError>;
    fn serialize_metadata(&self) -> Result<Vec<u8>, DbError>;

    fn snapshot(&self) -> Result<IndexSnapshot, DbError>;

    fn populate_vectors(&mut self, storage: &dyn StorageEngine) -> Result<(), DbError>;
}
