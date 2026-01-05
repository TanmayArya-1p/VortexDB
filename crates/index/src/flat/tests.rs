use super::index::FlatIndex;
use crate::{SerializableIndex, VectorIndex};
use defs::{IndexedVector, Similarity};
use uuid::Uuid;

#[test]
fn test_flat_index_new() {
    let index = FlatIndex::new();
    assert_eq!(index.index.len(), 0);
}

#[test]
fn test_flat_index_build() {
    let vectors = vec![
        IndexedVector {
            id: Uuid::new_v4(),
            vector: vec![1.0, 2.0, 3.0],
        },
        IndexedVector {
            id: Uuid::new_v4(),
            vector: vec![4.0, 5.0, 6.0],
        },
    ];
    let index = FlatIndex::build(vectors.clone());
    assert_eq!(index.index, vectors);
}

#[test]
fn test_insert() {
    let mut index = FlatIndex::new();
    let vector = IndexedVector {
        id: Uuid::new_v4(),
        vector: vec![1.0, 2.0, 3.0],
    };

    assert!(index.insert(vector.clone()).is_ok());
    assert_eq!(index.index.len(), 1);
    assert_eq!(index.index[0], vector);
}

#[test]
fn test_delete_existing() {
    let mut index = FlatIndex::new();
    let existing_id = Uuid::new_v4();
    let vector = IndexedVector {
        id: existing_id,
        vector: vec![1.0, 2.0, 3.0],
    };
    index.insert(vector).unwrap();

    let result = index.delete(existing_id).unwrap();
    assert!(result);
    assert_eq!(index.index.len(), 0);
}

#[test]
fn test_delete_non_existing() {
    let mut index = FlatIndex::new();
    let vector = IndexedVector {
        id: Uuid::new_v4(),
        vector: vec![1.0, 2.0, 3.0],
    };
    index.insert(vector).unwrap();

    let result = index.delete(Uuid::new_v4()).unwrap();
    assert!(!result);
    assert_eq!(index.index.len(), 1);
}

#[test]
fn test_search_euclidean() {
    let mut index = FlatIndex::new();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    index
        .insert(IndexedVector {
            id: id1,
            vector: vec![1.0, 1.0],
        })
        .unwrap();
    index
        .insert(IndexedVector {
            id: id2,
            vector: vec![2.0, 2.0],
        })
        .unwrap();
    index
        .insert(IndexedVector {
            id: id3,
            vector: vec![10.0, 10.0],
        })
        .unwrap();

    let results = index
        .search(vec![0.0, 0.0], Similarity::Euclidean, 2)
        .unwrap();
    assert_eq!(results, vec![id1, id2]);
}

#[test]
fn test_search_cosine() {
    let mut index = FlatIndex::new();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    index
        .insert(IndexedVector {
            id: id1,
            vector: vec![1.0, 0.0],
        })
        .unwrap();
    index
        .insert(IndexedVector {
            id: id2,
            vector: vec![0.5, 0.5],
        })
        .unwrap();
    index
        .insert(IndexedVector {
            id: id3,
            vector: vec![0.0, 1.0],
        })
        .unwrap();

    let results = index.search(vec![1.0, 1.0], Similarity::Cosine, 2).unwrap();
    assert_eq!(results, vec![id2, id1]);
}

#[test]
fn test_search_manhattan() {
    let mut index = FlatIndex::new();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    index
        .insert(IndexedVector {
            id: id1,
            vector: vec![1.0, 1.0],
        })
        .unwrap();
    index
        .insert(IndexedVector {
            id: id2,
            vector: vec![2.0, 2.0],
        })
        .unwrap();
    index
        .insert(IndexedVector {
            id: id3,
            vector: vec![5.0, 5.0],
        })
        .unwrap();

    let results = index
        .search(vec![0.0, 0.0], Similarity::Manhattan, 2)
        .unwrap();
    assert_eq!(results, vec![id1, id2]);
}

#[test]
fn test_search_hamming() {
    let mut index = FlatIndex::new();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    index
        .insert(IndexedVector {
            id: id1,
            vector: vec![1.0, 0.0, 1.0, 1.0],
        })
        .unwrap();
    index
        .insert(IndexedVector {
            id: id2,
            vector: vec![1.0, 0.0, 0.0, 0.0],
        })
        .unwrap();
    index
        .insert(IndexedVector {
            id: id3,
            vector: vec![0.0, 0.0, 0.0, 0.0],
        })
        .unwrap();

    let results = index
        .search(vec![1.0, 0.0, 0.0, 0.0], Similarity::Hamming, 2)
        .unwrap();
    assert_eq!(results, vec![id2, id3]);
}

#[test]
fn test_default() {
    let index = FlatIndex::default();
    assert_eq!(index.index.len(), 0);
}

#[test]
fn test_serialize_and_deserialize_topo() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    let id4 = Uuid::new_v4();

    let v1 = IndexedVector {
        id: id1,
        vector: vec![0.0, 0.0, 0.0, 0.0],
    };
    let v2 = IndexedVector {
        id: id2,
        vector: vec![1.0, 0.0, 0.0, 0.0],
    };
    let v3 = IndexedVector {
        id: id3,
        vector: vec![2.0, 0.0, 0.0, 0.0],
    };
    let v4 = IndexedVector {
        id: id4,
        vector: vec![3.0, 0.0, 0.0, 0.0],
    };

    let vectors = vec![v1.clone(), v2.clone(), v3.clone(), v4.clone()];
    let mut index_before = FlatIndex::build(vectors);
    index_before.insert(v4.clone()).unwrap();

    index_before.delete(id1).unwrap();

    let snapshot = index_before.snapshot().unwrap();

    let idx = FlatIndex::deserialize(&snapshot).unwrap();

    assert_eq!(idx.index.len(), 4);
    assert!(!idx.index.iter().any(|v| v.id == id1));
    assert!(idx.index.iter().any(|v| v.id == id2));
    assert!(idx.index.iter().any(|v| v.id == id3));
    assert!(idx.index.iter().any(|v| v.id == id3));
    assert!(idx.index.iter().any(|v| v.id == id4));
}
