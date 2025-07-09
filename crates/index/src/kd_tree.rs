use crate::{distance, VectorIndex};
use core::{DbError, DenseVector, IndexedVector, PointId, Similarity};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

#[derive(Debug, PartialEq)]
pub struct DataHeap {
    key: PointId,
    distance: f32,
}

// These traits must be implemented for a custom BinaryHeap
impl Eq for DataHeap {}
impl Ord for DataHeap {
    fn cmp(&self, other: &DataHeap) -> Ordering {
        self.distance
            .partial_cmp(&other.distance)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for DataHeap {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Serialize, Deserialize)]
pub struct KDTreeInternals {
    pub kd_tree_allow_update: bool,
    pub current_number_of_kd_tree_nodes: usize,
    pub rebuild_threshold: f32,
    pub previous_tree_size: usize,
    pub rebuild_counter: usize,
}

// Chaging dim to split_dim for verbosity
#[derive(Serialize, Deserialize)]
pub struct KDTreeNode {
    pub left: Option<Box<KDTreeNode>>,
    pub right: Option<Box<KDTreeNode>>,
    pub key: PointId,
    pub vector: DenseVector,
    pub split_dim: usize,
}

impl KDTreeNode {
    fn new(data: IndexedVector, dim: usize) -> KDTreeNode {
        KDTreeNode {
            left: None,
            right: None,
            key: data.id,
            vector: data.vector,
            split_dim: dim,
        }
    }

    pub fn find_nearest_neighbors<'a>(
        &'a self,
        point: &DenseVector,
        similarity: Similarity,
        heap: &'a mut BinaryHeap<DataHeap>,
        k: usize,
    ) -> usize {
        let mut stack = VecDeque::new();
        stack.push_back(self);
        let mut visited = 0;

        while let Some(node) = stack.pop_back() {
            visited += 1;

            let dist = distance(point, &node.vector, similarity);

            if heap.len() < k {
                heap.push(DataHeap {
                    key: node.key,
                    distance: dist,
                });
            } else if dist <= heap.peek().unwrap().distance {
                heap.pop();
                heap.push(DataHeap {
                    key: node.key,
                    distance: dist,
                });
            }

            let worst_dist = heap.peek().map(|d| d.distance).unwrap_or(f32::MAX);

            let plane_dist = (point[node.split_dim] - node.vector[node.split_dim]).abs();

            let (near_child, far_child) = if point[node.split_dim] < node.vector[node.split_dim] {
                (&node.left, &node.right)
            } else {
                (&node.right, &node.left)
            };

            if let Some(child) = near_child {
                stack.push_back(child);
            }

            if plane_dist <= worst_dist {
                if let Some(child) = far_child {
                    stack.push_back(child);
                }
            }
        }
        visited
    }

    pub fn print_kd_node(&self, depth: usize, is_left: bool) {
        let indent = "│   ".repeat(depth.saturating_sub(1))
            + if depth == 0 {
                ""
            } else if is_left {
                "├── "
            } else {
                "└── "
            };

        println!(
            "{}[id: {}, split_dim: {}, vec: {:?}]",
            indent, self.key, self.split_dim, self.vector
        );

        if let Some(ref left) = self.left {
            left.print_kd_node(depth + 1, true);
        }
        if let Some(ref right) = self.right {
            right.print_kd_node(depth + 1, false);
        }
    }
}

pub struct KDTree {
    pub _root: Option<Box<KDTreeNode>>,
    pub _internals: KDTreeInternals,
    pub is_debug_run: bool,
    pub dim: usize,
}

impl KDTree {
    // Create an empty tree with default values
    pub fn new(dim: usize) -> KDTree {
        KDTree {
            _root: None,
            _internals: KDTreeInternals {
                kd_tree_allow_update: true,
                current_number_of_kd_tree_nodes: 0,
                rebuild_threshold: 2.0f32,
                previous_tree_size: 0,
                rebuild_counter: 0,
            },
            is_debug_run: true,
            dim,
        }
    }

    pub fn debug_print(&self) {
        println!("KDTree Debug Print:");
        if let Some(ref root) = self._root {
            root.print_kd_node(0, true);
        } else {
            println!("(empty)");
        }
    }

    // Add a node
    pub fn add_node(&mut self, data: IndexedVector) -> Result<(), DbError> {
        // Validate dimensions
        if self.dim != data.vector.len() {
            return Err(DbError::IndexError(format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dim,
                data.vector.len()
            )));
        }

        // Locked for rebuild
        if !self._internals.kd_tree_allow_update {
            return Err(DbError::IndexError(
                "KD Tree locked for rebuild".to_string(),
            ));
        }

        // Rebuild checks
        if self._internals.current_number_of_kd_tree_nodes > 0
            && self._internals.previous_tree_size > 0
        {
            let current_ratio = self._internals.current_number_of_kd_tree_nodes as f32
                / self._internals.previous_tree_size as f32;

            if current_ratio > self._internals.rebuild_threshold {
                self.rebuild()?;
            }
        }

        let mut depth = 0;

        if self._root.is_none() {
            self.dim = data.vector.len();
            self._root = Some(Box::new(KDTreeNode::new(data, 0)));
            self._internals.current_number_of_kd_tree_nodes = 1;
            self._internals.previous_tree_size = 1;
            return Ok(());
        }

        let mut current = self._root.as_deref_mut().unwrap();
        loop {
            let dim = depth % self.dim;
            depth += 1;

            if data.vector[dim] < current.vector[dim] {
                match current.left {
                    Some(ref mut left) => {
                        current = left;
                    }
                    None => {
                        current.left = Some(Box::new(KDTreeNode::new(data, dim)));
                        break;
                    }
                }
            } else {
                match current.right {
                    Some(ref mut right) => {
                        current = right;
                    }
                    None => {
                        current.right = Some(Box::new(KDTreeNode::new(data, dim)));
                        break;
                    }
                }
            }
        }

        self._internals.current_number_of_kd_tree_nodes += 1;

        self.debug_print();
        Ok(())
    }

    fn rebuild(&mut self) -> Result<(), DbError> {
        // TODO: Move this to update?
        if !self._internals.kd_tree_allow_update {
            return Err(DbError::IndexError(
                "KD Tree locked for rebuild".to_string(),
            ));
        }

        self._internals.kd_tree_allow_update = false;
        self._internals.rebuild_counter += 1;

        let points = self.traversal();
        self._root = create_tree_helper(&points, 0);
        self._internals.previous_tree_size = points.len();
        self._internals.current_number_of_kd_tree_nodes = points.len();
        self._internals.kd_tree_allow_update = true;

        Ok(())
    }

    pub fn traversal(&self) -> Vec<IndexedVector> {
        let mut result = Vec::new();
        let mut stack = Vec::new();
        let mut current = self._root.as_deref();
        // let mut stack = vec![self._root.as_deref()];

        while !stack.is_empty() || current.is_some() {
            while let Some(node) = current {
                stack.push(node);
                current = node.left.as_deref();
            }
            if let Some(node) = stack.pop() {
                result.push(IndexedVector {
                    id: node.key,
                    vector: node.vector.clone(),
                });
                current = node.right.as_deref();
            }
        }

        result
    }
}

impl VectorIndex for KDTree {
    fn insert(&mut self, vector: IndexedVector) -> Result<(), DbError> {
        self.add_node(vector)
    }

    fn search(
        &self,
        query_vector: DenseVector,
        similarity: Similarity,
        k: usize,
    ) -> Result<Vec<PointId>, DbError> {
        if k == 0 || self._root.is_none() {
            return Ok(Vec::new());
        }

        // Initalize a heap with worst possible values
        let mut heap = BinaryHeap::with_capacity(k);

        self._root.as_ref().unwrap().find_nearest_neighbors(
            &query_vector,
            similarity,
            &mut heap,
            k,
        );

        let mut results = Vec::with_capacity(heap.len());

        while let Some(item) = heap.pop() {
            results.push(item.key);
        }
        results.reverse();

        Ok(results)
    }

    fn delete(&mut self, point_id: PointId) -> Result<bool, DbError> {
        // Locked for rebuild
        if !self._internals.kd_tree_allow_update {
            return Err(DbError::IndexError(
                "KD Tree locked for rebuild".to_string(),
            ));
        }

        let mut points = self.traversal();
        let original_len = points.len();

        points.retain(|iv| iv.id != point_id);

        if points.len() == original_len {
            return Ok(false);
        }

        self._root = create_tree_helper(&points, 0);
        self._internals.current_number_of_kd_tree_nodes = points.len();
        Ok(true)
    }
}

// Rebuild tree helper functions
fn create_tree_helper(points: &[IndexedVector], depth: usize) -> Option<Box<KDTreeNode>> {
    if points.is_empty() {
        return None;
    }

    let dim = depth % points[0].vector.len();
    let mut sorted = points.to_vec();

    // Sort by the current dimension before finding median
    sorted.sort_unstable_by(|a, b| {
        a.vector[dim]
            .partial_cmp(&b.vector[dim])
            .unwrap_or(Ordering::Equal)
    });

    let mid = sorted.len() / 2;
    let median = sorted[mid].clone();
    let next_depth = depth + 1;

    Some(Box::new(KDTreeNode {
        key: median.id,
        vector: median.vector,
        split_dim: dim,
        left: create_tree_helper(&sorted[..mid], next_depth),
        right: create_tree_helper(&sorted[mid + 1..], next_depth),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_kd_tree() {
        let tree = KDTree::new(3);
        assert!(tree._root.is_none());
        assert_eq!(tree.dim, 3);
        assert_eq!(tree._internals.current_number_of_kd_tree_nodes, 0);
    }

    #[test]
    fn test_add_node() {
        let mut tree = KDTree::new(3);
        let vector = IndexedVector {
            id: 1,
            vector: vec![1.0, 2.0, 3.0],
        };

        // Add first node
        let result = tree.add_node(vector);
        assert!(result.is_ok());
        assert_eq!(tree._internals.current_number_of_kd_tree_nodes, 1);
        assert!(tree._root.is_some());

        // Add second node to the left
        let vector2 = IndexedVector {
            id: 2,
            vector: vec![0.0, 2.0, 3.0],
        };
        let result = tree.add_node(vector2);
        assert!(result.is_ok());
        assert_eq!(tree._internals.current_number_of_kd_tree_nodes, 2);

        // Add third node to the right
        let vector3 = IndexedVector {
            id: 3,
            vector: vec![2.0, 2.0, 3.0],
        };
        let result = tree.add_node(vector3);
        assert!(result.is_ok());
        assert_eq!(tree._internals.current_number_of_kd_tree_nodes, 3);
    }

    #[test]
    fn test_traversal() {
        let mut tree = KDTree::new(2);
        tree.insert(IndexedVector {
            id: 1,
            vector: vec![2.0, 3.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 2,
            vector: vec![5.0, 4.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 3,
            vector: vec![9.0, 6.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 4,
            vector: vec![4.0, 7.0],
        })
        .unwrap();

        let result = tree.traversal();
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_delete_node() {
        let mut tree = KDTree::new(2);
        tree.insert(IndexedVector {
            id: 1,
            vector: vec![2.0, 3.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 2,
            vector: vec![5.0, 4.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 3,
            vector: vec![9.0, 6.0],
        })
        .unwrap();

        // Delete existing node
        let result = tree.delete(2);
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Check node is deleted
        let traversal = tree.traversal();
        assert_eq!(traversal.len(), 2);
        assert!(!traversal.iter().any(|v| v.id == 2));

        // Delete non-existent node
        let result = tree.delete(10);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_search() {
        let mut tree = KDTree::new(2);
        tree.insert(IndexedVector {
            id: 1,
            vector: vec![1.0, 1.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 2,
            vector: vec![2.0, 2.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 3,
            vector: vec![3.0, 3.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 4,
            vector: vec![4.0, 4.0],
        })
        .unwrap();

        // Search for nearest neighbor
        let result = tree.search(vec![1.1, 1.1], Similarity::Euclidean, 1);
        assert!(result.is_ok());
        let nearest = result.unwrap();
        assert_eq!(nearest.len(), 1);
        assert_eq!(nearest[0], 1);

        // Search for multiple neighbors
        let result = tree.search(vec![2.5, 2.5], Similarity::Euclidean, 2);
        assert!(result.is_ok());
        let nearest = result.unwrap();
        println!("nearest: {:?}", nearest);
        assert_eq!(nearest.len(), 2);

        assert!(nearest.contains(&2) && nearest.contains(&3));
    }

    #[test]
    fn test_search_with_various_similarities() {
        let mut tree = KDTree::new(2);
        tree.insert(IndexedVector {
            id: 1,
            vector: vec![1.0, 1.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 2,
            vector: vec![2.0, 2.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 3,
            vector: vec![3.0, 3.0],
        })
        .unwrap();

        let result = tree.search(vec![1.1, 1.1], Similarity::Euclidean, 1);
        assert!(result.is_ok());
        let nearest = result.unwrap();
        assert_eq!(nearest.len(), 1);
        assert_eq!(nearest[0], 1); // [1.0, 1.0] is closest by Euclidean distance

        let result = tree.search(vec![1.0, 1.0], Similarity::Cosine, 1);
        assert!(result.is_ok());
        let nearest = result.unwrap();
        assert_eq!(nearest.len(), 1);
        // Should be one of the vectors in the same direction [1,1], [2,2], or [3,3]
        assert!(nearest[0] == 1 || nearest[0] == 2 || nearest[0] == 3);

        let result = tree.search(vec![1.1, 1.1], Similarity::Manhattan, 1);
        assert!(result.is_ok());
        let nearest = result.unwrap();
        assert_eq!(nearest.len(), 1);
        assert_eq!(nearest[0], 1); // [1.0, 1.0] is closest by Manhattan distance
    }

    #[test]
    fn test_search_edge_cases() {
        let mut tree = KDTree::new(3);

        // Test on empty tree
        let result = tree.search(vec![1.0, 2.0, 3.0], Similarity::Euclidean, 5);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);

        // Add some points
        for i in 1..6 {
            tree.insert(IndexedVector {
                id: i,
                vector: vec![i as f32, i as f32, i as f32],
            })
            .unwrap();
        }

        // Test k=0
        let result = tree.search(vec![2.5, 2.5, 2.5], Similarity::Euclidean, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);

        // Test k > number of points
        let result = tree.search(vec![2.5, 2.5, 2.5], Similarity::Euclidean, 10);
        assert!(result.is_ok());
        let nearest = result.unwrap();
        assert_eq!(nearest.len(), 5); // Should return all 5 points
    }

    #[test]
    fn test_search_high_dimensional_vectors() {
        let mut tree = KDTree::new(5);

        let vectors = vec![
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            vec![2.0, 3.0, 4.0, 5.0, 6.0],
            vec![3.0, 4.0, 5.0, 6.0, 7.0],
            vec![4.0, 5.0, 6.0, 7.0, 8.0],
            vec![5.0, 6.0, 7.0, 8.0, 9.0],
            vec![1.5, 2.5, 3.5, 4.5, 5.5],
            vec![2.5, 3.5, 4.5, 5.5, 6.5],
            vec![3.5, 4.5, 5.5, 6.5, 7.5],
            vec![4.5, 5.5, 6.5, 7.5, 8.5],
            vec![5.5, 6.5, 7.5, 8.5, 9.5],
        ];

        for (i, v) in vectors.iter().enumerate() {
            tree.insert(IndexedVector {
                id: i as u64 + 1,
                vector: v.clone(),
            })
            .unwrap();
        }

        // A query close to the first few vectors
        let query = vec![1.4, 2.4, 3.4, 4.4, 5.4];

        // Euclidean
        let result = tree.search(query.clone(), Similarity::Euclidean, 3);
        assert!(result.is_ok());
        let nearest = result.unwrap();
        assert_eq!(nearest.len(), 3);
        assert_eq!(nearest[0], 6); // [1.5,2.5,...] should be the closest
    }

    #[test]
    fn test_search_with_identical_vectors() {
        let mut tree = KDTree::new(2);

        // Insert multiple identical vectors
        tree.insert(IndexedVector {
            id: 1,
            vector: vec![1.0, 1.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 2,
            vector: vec![1.0, 1.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 3,
            vector: vec![1.0, 1.0],
        })
        .unwrap();

        // Also add some different vectors
        tree.insert(IndexedVector {
            id: 4,
            vector: vec![2.0, 2.0],
        })
        .unwrap();
        tree.insert(IndexedVector {
            id: 5,
            vector: vec![3.0, 3.0],
        })
        .unwrap();

        // Search for nearest neighbors to an identical point
        let result = tree.search(vec![1.0, 1.0], Similarity::Euclidean, 3);
        assert!(result.is_ok());
        let nearest = result.unwrap();
        assert_eq!(nearest.len(), 3);

        // All three should be from the identical vectors
        for id in nearest {
            assert!(id == 1 || id == 2 || id == 3);
        }
    }

    #[test]
    fn test_search_performance_larger_dataset() {
        let mut tree = KDTree::new(5);
        let mut point_ids = Vec::new();

        // Insert 100 vectors
        for i in 0..100 {
            let id = i + 1;
            let vector = vec![
                (i % 10) as f32,
                ((i + 7) % 10) as f32,
                ((i + 3) % 10) as f32,
                ((i + 5) % 10) as f32,
                ((i + 1) % 10) as f32,
            ];
            tree.insert(IndexedVector { id, vector }).unwrap();
            point_ids.push(id);
        }

        // Search for k=10 nearest neighbors
        let result = tree.search(vec![5.0, 5.0, 5.0, 5.0, 5.0], Similarity::Euclidean, 10);
        assert!(result.is_ok());
        let nearest = result.unwrap();
        assert_eq!(nearest.len(), 10);

        // All results should be valid point ids
        for id in nearest {
            assert!(point_ids.contains(&id));
        }
    }

    #[test]
    fn test_data_heap() {
        let mut heap = BinaryHeap::new();

        heap.push(DataHeap {
            key: 1,
            distance: 3.0,
        });
        heap.push(DataHeap {
            key: 2,
            distance: 1.0,
        });
        heap.push(DataHeap {
            key: 3,
            distance: 2.0,
        });

        // In a max heap with our custom ordering, the largest distance is at the top
        assert_eq!(heap.peek().unwrap().key, 1);
        assert_eq!(heap.peek().unwrap().distance, 3.0);

        // Pop items and verify they come in decreasing distance order
        let item = heap.pop().unwrap();
        assert_eq!(item.key, 1);
        assert_eq!(item.distance, 3.0);

        let item = heap.pop().unwrap();
        assert_eq!(item.key, 3);
        assert_eq!(item.distance, 2.0);

        let item = heap.pop().unwrap();
        assert_eq!(item.key, 2);
        assert_eq!(item.distance, 1.0);

        assert!(heap.is_empty());
    }

    #[test]
    fn test_rebuild() {
        let mut tree = KDTree::new(2);

        // Add enough nodes to trigger a rebuild
        for i in 0..10 {
            tree.insert(IndexedVector {
                id: i,
                vector: vec![i as f32, (i * 2) as f32],
            })
            .unwrap();
        }

        // Check rebuild occurred
        assert!(tree._internals.rebuild_counter > 0);

        // Tree should still contain all points after rebuild
        let traversal = tree.traversal();
        assert_eq!(traversal.len(), 10);
    }
}
