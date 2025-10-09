use serde::{Deserialize, Serialize};

pub type PointId = u64;

/// Type of vector element.
pub type Element = f32;
// pub type ElementHalf = f16; - Unstable https://github.com/rust-lang/rust/issues/116909
pub type ElementByte = u8;

// Dense Vector and Vector are considered same
// Sparse vector implementation not supported yet. Refer lib/sparse/src/common/sparse_vector.rs
pub type DenseVector = Vec<Element>;

pub enum StoredVector {
    Dense(DenseVector),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Payload {
    // Define here how payload is managed
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Point {
    pub id: PointId,
    pub vector: Option<DenseVector>,
    pub payload: Option<Payload>,
}

/// Struct which will be stored in the vector index
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct IndexedVector {
    pub id: PointId,
    pub vector: DenseVector,
}

#[derive(Copy, Clone)]
pub enum Similarity {
    Euclidean,
    Manhattan,
    Hamming,
    Cosine,
}

// Query Vector. Basically the type of query results that can be generated. Not implementing this but referencing here for furture reference
// #[derive(Debug, Clone)]
// pub enum QueryVector {
//     Nearest(VectorInternal),
//     RecommendBestScore(RecoQuery<VectorInternal>),
//     RecommendSumScores(RecoQuery<VectorInternal>),
//     Discovery(DiscoveryQuery<VectorInternal>),
//     Context(ContextQuery<VectorInternal>),
// }
