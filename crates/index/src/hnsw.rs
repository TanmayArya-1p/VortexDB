use crate::{distance, VectorIndex};
use core::{DbError, DenseVector, IndexedVector, PointId, Similarity};
use std::collections::{HashMap, HashSet};

use rand::Rng;

// Compact storage for layered points and adjacency used by `HnswIndex`.
pub struct PointIndexation {
    // Max connections per point per layer (M)
    pub max_nb_connection: usize,
    // Maximum number of layers
    pub max_layer: usize,
    // Points per layer; each inner Vec holds the PointId(s)
    pub points_by_layer: Vec<Vec<PointId>>,
    // Per-node, per-level neighbor lists (bounded by M/M0)
    pub nodes: HashMap<PointId, Node>,
    // Number of points inserted
    pub nb_point: usize,
    // Optional entry point used for searches/insertions
    pub entry_point: Option<PointId>,
    // Level generator used to sample random levels
    pub level_generator: LevelGenerator,
}

// Node with highest level and per-level neighbor lists
pub struct Node {
    pub id: PointId,
    // Highest level (0-based; level 0 is the base layer)
    pub level: u8,
    // neighbors[level] -> neighbor PointIds at that level
    pub neighbors: Vec<Vec<PointId>>,
    // Soft-delete flag (skipped by traversals)
    pub deleted: bool,
}

// Level sampling parameters
pub struct LevelGenerator {
    // 1 / ln(M)
    pub level_scale: f64,
    // Maximum number of layers
    pub max_layer: usize,
}

impl LevelGenerator {
    pub fn from_m(m: usize, max_layer: usize) -> Self {
        assert!(m >= 2, "LevelGenerator::from_m: m must be >= 2");
        let level_scale = 1.0 / (m as f64).ln();
        Self {
            level_scale,
            max_layer,
        }
    }

    /// Sample a level `L` from an exponential tail: P(L ≥ l) ≈ exp(-l / ln M).
    /// Uses inverse transform: L = floor(-ln(U) * (1/ln M)), capped to `max_layer - 1`.
    pub fn sample_level<R: Rng>(&self, rng: &mut R) -> u8 {
        let mut u: f64 = rng.gen();
        if u <= 0.0 {
            u = f64::EPSILON;
        }
        if u >= 1.0 {
            u = 1.0 - f64::EPSILON;
        }

        let raw = (-u.ln()) * self.level_scale;
        let l = raw.floor() as usize;
        let capped = l.min(self.max_layer.saturating_sub(1));
        capped as u8
    }
}

/// Referenced from HNSW (Malkov & Yashunin, 2018)
/// https://arxiv.org/abs/1603.09320
pub struct HnswIndex {
    // Construction/search parameters
    pub ef_construction: usize,
    // Max edges per node on upper layers (M)
    pub max_connections: usize,
    // Max edges per node on layer 0 (often 2*M)
    pub max_connections_0: usize,
    // Heuristic flags controlling neighbor selection/pruning
    pub extend_candidates: bool,
    pub keep_pruned: bool,
    // Layering parameters
    pub max_layer: usize,
    // Layered point storage and entry point
    pub index: PointIndexation,
    // Cached dimension of stored vectors
    pub data_dimension: usize,
    // Guard against concurrent mutation during queries
    pub searching: bool,
    // Default query beam width (ef); recommended ef ≥ k at query time
    pub ef: usize,
    // In-memory vector cache owned by the index
    cache: HashMap<PointId, DenseVector>,
    // Fixed metric for this index; used consistently in insert and search
    pub similarity: Similarity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HnswStats {
    pub alive: usize,
    pub deleted: usize,
    pub level_histogram: std::collections::BTreeMap<u8, usize>,
}

impl HnswIndex {
    pub fn new(similarity: Similarity) -> Self {
        let max_connections = 16; // M
        let max_connections_0 = 32; // M0 = 2 * M (common default)
        let max_layer = 16; // implementation cap
        let ef_construction = 200;
        let ef = 100;
        let extend_candidates = false;
        let keep_pruned = false;

        let level_generator = LevelGenerator::from_m(max_connections, max_layer);
        let index = PointIndexation {
            max_nb_connection: max_connections,
            max_layer,
            points_by_layer: vec![Vec::new(); max_layer],
            nodes: HashMap::new(),
            nb_point: 0,
            entry_point: None,
            level_generator,
        };

        Self {
            ef_construction,
            max_connections,
            max_connections_0,
            extend_candidates,
            keep_pruned,
            max_layer,
            index,
            data_dimension: 0,
            searching: false,
            ef,
            cache: HashMap::new(),
            similarity,
        }
    }

    /// Returns a slice of the stored vector for the given PointId.
    /// TODO: integrate this cache with an in-memory store backed by RocksDB; on cache miss,
    /// fetch from storage, populate the cache, and return a stable slice.
    fn get_vec(&self, id: PointId) -> &[f32] {
        let v = self
            .cache
            .get(&id)
            .unwrap_or_else(|| panic!("Vector not found in HNSW cache for id={id}"));
        v.as_slice()
    }
}

impl VectorIndex for HnswIndex {
    /// Insert a new point
    /// - sample a random level for the new node
    /// - if empty, set entry point to the new id and return
    /// - greedy descend from current entry to l+1 to get a pivot
    /// - for each level down to 0: ef-construction, diversity pruning, bidirectional connect with caps
    /// - if l is above current max level, update the entry point
    fn insert(&mut self, vector: IndexedVector) -> Result<(), DbError> {
        let dim = vector.vector.len();
        if self.data_dimension == 0 {
            self.data_dimension = dim;
        } else {
            debug_assert_eq!(
                self.data_dimension, dim,
                "HNSW insert: vector dimension {} != index dimension {}",
                dim, self.data_dimension
            );
        }

        let new_id: PointId = vector.id;

        let mut query_vec = vector.vector.clone();
        if let Similarity::Cosine = self.similarity {
            let mut sum_sq = 0.0f32;
            for &x in &query_vec {
                sum_sq += x * x;
            }
            let norm = sum_sq.sqrt();
            if norm > 0.0 {
                for x in &mut query_vec {
                    *x /= norm;
                }
            }
        }

        self.cache.insert(new_id, query_vec.clone());

        let mut rng = rand::thread_rng();
        let l: u8 = self.index.level_generator.sample_level(&mut rng);

        let node = Node {
            id: new_id,
            level: l,
            neighbors: vec![vec![]; (l as usize) + 1],
            deleted: false,
        };

        let needed_layers = (l as usize) + 1;
        if self.index.points_by_layer.len() < needed_layers {
            self.index.points_by_layer.resize(needed_layers, Vec::new());
        }
        self.index.nodes.insert(new_id, node);
        for layer in 0..=l as usize {
            self.index.points_by_layer[layer].push(new_id);
        }

        if self.index.entry_point.is_none() {
            self.index.entry_point = Some(new_id);
            return Ok(());
        }

        let mut ep = self.index.entry_point.unwrap();
        let current_max_level = self
            .index
            .nodes
            .get(&ep)
            .map(|n| n.level as usize)
            .unwrap_or(0);
        if current_max_level > (l as usize) {
            for level in ((l as usize + 1)..=current_max_level).rev() {
                ep = self.greedy_search_layer(ep, level, &query_vec);
            }
        }

        for level in (0..=std::cmp::min(l as usize, current_max_level)).rev() {
            let w = self.search_layer_for_insert(ep, level, &query_vec, self.ef_construction);

            let m_level = if level == 0 {
                self.max_connections_0
            } else {
                self.max_connections
            };
            let chosen = self.select_neighbors_heuristic(&w, m_level);
            self.connect_bidirectional(new_id, &chosen, level, m_level);
            if let Some((closest_id, _)) = w.first() {
                ep = *closest_id;
            }
        }
        if (l as usize) > current_max_level {
            self.index.entry_point = Some(new_id);
        }

        Ok(())
    }

    /// Delete a point (soft)
    /// - mark node as deleted and clear its cached vector
    /// - traversals skip deleted nodes
    /// - if entry point was deleted, move it to the highest-level non-deleted node (or None)
    fn delete(&mut self, point_id: PointId) -> Result<bool, DbError> {
        if let Some(node) = self.index.nodes.get_mut(&point_id) {
            if node.deleted {
                return Ok(false);
            }
            node.deleted = true;
            self.cache.remove(&point_id);
            if self.index.entry_point == Some(point_id) {
                if let Some((&new_ep, _)) = self
                    .index
                    .nodes
                    .iter()
                    .filter(|(_, nd)| !nd.deleted)
                    .max_by(|a, b| a.1.level.cmp(&b.1.level).then_with(|| a.0.cmp(b.0)))
                {
                    self.index.entry_point = Some(new_ep);
                } else {
                    self.index.entry_point = None;
                }
            }
            return Ok(true);
        }
        Ok(false)
    }
    /// Search for top-k ids
    /// - normalize query for cosine (1 − cos)
    /// - pick a non-deleted entry point
    /// - greedy descend from the top layer to level 1
    /// - run ef-best-first at level 0 with ef0 = max(ef, k)
    /// - return up to k ids by ascending distance
    fn search(
        &self,
        mut query: std::vec::Vec<f32>,
        _similarity: Similarity,
        k: usize,
    ) -> Result<Vec<PointId>, DbError> {
        use std::cmp::max;
        if k == 0 {
            return Ok(Vec::new());
        }
        debug_assert!(
            self.data_dimension == 0 || query.len() == self.data_dimension,
            "HNSW search: query dimension {} != index dimension {}",
            query.len(),
            self.data_dimension
        );
        let entry = match self.index.entry_point {
            Some(id) => {
                if let Some(n) = self.index.nodes.get(&id) {
                    if n.deleted {
                        None
                    } else {
                        Some(id)
                    }
                } else {
                    None
                }
            }
            None => None,
        }
        .or_else(|| {
            self.index
                .nodes
                .iter()
                .filter(|(_, n)| !n.deleted)
                .max_by(|a, b| a.1.level.cmp(&b.1.level).then_with(|| a.0.cmp(b.0)))
                .map(|(id, _)| *id)
        });
        let entry = match entry {
            Some(id) => id,
            None => return Ok(Vec::new()),
        };
        if matches!(self.similarity, Similarity::Cosine) {
            let norm = query
                .iter()
                .map(|x| (*x as f64) * (*x as f64))
                .sum::<f64>()
                .sqrt();
            if norm > 0.0 {
                for v in &mut query {
                    *v /= norm as f32;
                }
            }
        }
        let mut ep = entry;
        let current_max_level = self
            .index
            .nodes
            .get(&entry)
            .map(|n| n.level as usize)
            .unwrap_or(0);
        if current_max_level > 0 {
            for level in (1..=current_max_level).rev() {
                ep = self.greedy_search_layer(ep, level, &query);
            }
        }
        let ef0 = max(self.ef, k);
        let mut w = self.search_layer_for_insert(ep, 0, &query, ef0);
        w.truncate(k);
        let result: Vec<u64> = w.into_iter().map(|(id, _)| id).collect();
        Ok(result)
    }
}

impl HnswIndex {
    /// Full rebuild from surviving (non-deleted) vectors currently in-memory.
    /// Gathers all non-deleted vectors from the cache, clears the graph, and reinserts.
    /// For persistence-backed deployments, prefer rebuilding from the storage layer.
    pub fn rebuild_full(&mut self) -> Result<(), DbError> {
        let ids: Vec<PointId> = self
            .index
            .nodes
            .iter()
            .filter(|(_, n)| !n.deleted)
            .map(|(id, _)| *id)
            .collect();
        let mut points: Vec<IndexedVector> = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(vec) = self.cache.get(&id) {
                points.push(IndexedVector {
                    id,
                    vector: vec.clone(),
                });
            } else {
                continue;
            }
        }
        self.index.nodes.clear();
        for layer in &mut self.index.points_by_layer {
            layer.clear();
        }
        self.index.nb_point = 0;
        self.index.entry_point = None;
        self.cache.clear();
        for iv in points {
            self.insert(iv)?;
        }
        Ok(())
    }
}

impl HnswIndex {
    /// Greedy search within a fixed layer
    /// - start from `ep` and evaluate neighbors at `level`
    /// - move to a neighbor only if it strictly improves distance
    /// - stop when no improvement and return the last id
    fn greedy_search_layer(&self, ep: PointId, level: usize, query: &[f32]) -> PointId {
        let mut current = ep;
        loop {
            let cur_vec = self.get_vec(current);
            let mut best_score = distance(query.to_vec(), cur_vec.to_vec(), self.similarity);
            let mut best_id = current;

            let empty: &[PointId] = &[];
            let neighbors = self
                .index
                .nodes
                .get(&current)
                .and_then(|n| n.neighbors.get(level))
                .map(|v| v.as_slice())
                .unwrap_or(empty);

            for &n in neighbors {
                if n == current {
                    continue;
                }
                // Skip deleted neighbors
                if let Some(nn) = self.index.nodes.get(&n) {
                    if nn.deleted {
                        continue;
                    }
                }
                let n_vec = self.get_vec(n);
                let score = distance(query.to_vec(), n_vec.to_vec(), self.similarity);
                if score < best_score {
                    best_score = score;
                    best_id = n;
                }
            }

            if best_id == current {
                break;
            }
            current = best_id;
        }
        current
    }

    /// Fraction of deleted nodes among all nodes (0.0 if no nodes)
    pub fn deleted_ratio(&self) -> f32 {
        let total = self.index.nodes.len();
        if total == 0 {
            return 0.0;
        }
        let deleted = self.index.nodes.values().filter(|n| n.deleted).count();
        deleted as f32 / total as f32
    }

    /// alive/deleted counts and level histogram for alive nodes
    pub fn stats(&self) -> HnswStats {
        let mut alive = 0usize;
        let mut deleted = 0usize;
        let mut hist: std::collections::BTreeMap<u8, usize> = std::collections::BTreeMap::new();
        for n in self.index.nodes.values() {
            if n.deleted {
                deleted += 1;
            } else {
                alive += 1;
                *hist.entry(n.level).or_insert(0) += 1;
            }
        }
        HnswStats {
            alive,
            deleted,
            level_histogram: hist,
        }
    }
}

impl HnswIndex {
    /// Best-first (ef) search used during insertion on a given layer
    /// - maintain candidate queue and working set up to `ef_construction`
    /// - expand the closest candidate; skip deleted nodes
    /// - early-exit if the best candidate is worse than the worst in W when full
    /// - return W as (id, distance) sorted by ascending distance
    fn search_layer_for_insert(
        &self,
        ep: PointId,
        level: usize,
        query: &[f32],
        ef_construction: usize,
    ) -> Vec<(PointId, f32)> {
        let mut visited: HashSet<PointId> = HashSet::new();
        let mut candidates: Vec<(f32, PointId)> = Vec::new();
        let mut w: Vec<(f32, PointId)> = Vec::new();

        // Seed with a non-deleted entry point
        let seed = if let Some(node) = self.index.nodes.get(&ep) {
            if node.deleted {
                self.index
                    .nodes
                    .iter()
                    .filter(|(_, n)| !n.deleted && n.neighbors.len() > level)
                    .max_by(|a, b| a.1.level.cmp(&b.1.level).then_with(|| a.0.cmp(b.0)))
                    .map(|(id, _)| *id)
                    .unwrap_or(ep)
            } else {
                ep
            }
        } else {
            ep
        };
        let ep_score = distance(query.to_vec(), self.get_vec(seed).to_vec(), self.similarity);
        candidates.push((ep_score, seed));
        w.push((ep_score, seed));
        visited.insert(seed);

        while !candidates.is_empty() {
            let (best_idx, (best_score, best_id)) = candidates
                .iter()
                .enumerate()
                .min_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap())
                .map(|(i, v)| (i, *v))
                .unwrap();
            candidates.swap_remove(best_idx);

            if w.len() >= ef_construction {
                if let Some((_, (worst_score, _))) = w
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap())
                {
                    if best_score > *worst_score {
                        break;
                    }
                }
            }

            let empty: &[PointId] = &[];
            let neighbors = self
                .index
                .nodes
                .get(&best_id)
                .and_then(|n| n.neighbors.get(level))
                .map(|v| v.as_slice())
                .unwrap_or(empty);
            for &n in neighbors {
                if visited.contains(&n) {
                    continue;
                }
                // Skip deleted neighbors
                if let Some(nn) = self.index.nodes.get(&n) {
                    if nn.deleted {
                        continue;
                    }
                }
                visited.insert(n);
                let score = distance(query.to_vec(), self.get_vec(n).to_vec(), self.similarity);
                candidates.push((score, n));
                if w.len() < ef_construction {
                    w.push((score, n));
                } else if let Some((worst_idx, (worst_score, _))) = w
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap())
                {
                    if score < *worst_score {
                        w[worst_idx] = (score, n);
                    }
                }
            }
        }

        let mut out: Vec<(PointId, f32)> = w.into_iter().map(|(s, id)| (id, s)).collect();
        out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        out
    }
}

impl HnswIndex {
    /// Diversity-based neighbor selection (heuristic pruning)
    /// - sort candidates by distance-to-new ascending
    /// - accept a candidate unless it is dominated by an accepted one
    /// - return up to `m` ids
    fn select_neighbors_heuristic(&self, candidates: &[(PointId, f32)], m: usize) -> Vec<PointId> {
        let mut sorted = candidates.to_vec();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let mut result: Vec<PointId> = Vec::with_capacity(m);

        'outer: for &(cand_id, cand_dist_to_q) in &sorted {
            let cand_vec = self.get_vec(cand_id);
            for &r_id in &result {
                let r_vec = self.get_vec(r_id);
                let cand_to_r = distance(cand_vec.to_vec(), r_vec.to_vec(), self.similarity);
                if cand_to_r < cand_dist_to_q {
                    continue 'outer;
                }
            }
            result.push(cand_id);
            if result.len() >= m {
                break;
            }
        }

        result
    }
}

impl HnswIndex {
    /// Connect new node `p` with `neighbors` on `level`
    /// - ensure level storage exists
    /// - merge and prune neighbor lists for `p` and each neighbor (cap by `m`/`M0`)
    /// - skip linking into deleted nodes
    fn connect_bidirectional(&mut self, p: PointId, neighbors: &[PointId], level: usize, m: usize) {
        let node = self.index.nodes.get_mut(&p).expect("node must exist");
        if node.neighbors.len() <= level {
            node.neighbors.resize(level + 1, Vec::new());
        }

        let mut combined_p: Vec<PointId> = {
            let node = self.index.nodes.get(&p).unwrap();
            node.neighbors[level].clone()
        };
        for &n in neighbors {
            if n != p && !combined_p.contains(&n) {
                combined_p.push(n);
            }
        }
        let p_vec = self.get_vec(p).to_vec();
        let mut scored_p: Vec<(PointId, f32)> = combined_p
            .into_iter()
            .map(|nid| {
                let d = distance(p_vec.clone(), self.get_vec(nid).to_vec(), self.similarity);
                (nid, d)
            })
            .collect();
        scored_p.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        scored_p.truncate(m);
        let new_p_list: Vec<PointId> = scored_p.into_iter().map(|(nid, _)| nid).collect();
        {
            let node = self.index.nodes.get_mut(&p).unwrap();
            node.neighbors[level] = new_p_list;
        }

        for &n in neighbors {
            if n == p {
                continue;
            }
            if let Some(nn) = self.index.nodes.get(&n) {
                if nn.deleted {
                    continue;
                }
            }
            {
                let node = self.index.nodes.get_mut(&n).expect("neighbor must exist");
                if node.neighbors.len() <= level {
                    node.neighbors.resize(level + 1, Vec::new());
                }
            }
            let cap = if level == 0 {
                self.max_connections_0
            } else {
                self.max_connections
            };

            let mut combined_n: Vec<PointId> = {
                let node = self.index.nodes.get(&n).unwrap();
                node.neighbors[level].clone()
            };
            if !combined_n.contains(&p) {
                combined_n.push(p);
            }

            let n_vec = self.get_vec(n).to_vec();
            let mut scored_n: Vec<(PointId, f32)> = combined_n
                .into_iter()
                .map(|nid| {
                    let d = distance(n_vec.clone(), self.get_vec(nid).to_vec(), self.similarity);
                    (nid, d)
                })
                .collect();
            scored_n.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            scored_n.truncate(cap);
            let new_n_list: Vec<PointId> = scored_n.into_iter().map(|(nid, _)| nid).collect();
            {
                let node = self.index.nodes.get_mut(&n).unwrap();
                node.neighbors[level] = new_n_list;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat::FlatIndex;
    use crate::VectorIndex;
    use core::IndexedVector;

    #[test]
    fn test_entry_point_after_first_insert() {
        let mut index = HnswIndex::new(Similarity::Euclidean);
        let v1 = IndexedVector {
            id: 1,
            vector: vec![1.0, 0.0],
        };
        assert!(index.insert(v1).is_ok());

        // Entry point should be set to the first inserted id
        assert_eq!(index.index.entry_point, Some(1));
        // Layer 0 should contain the point
        assert!(index.index.points_by_layer.first().unwrap().contains(&1));
    }

    #[test]
    fn test_connectivity_level0_after_two_inserts() {
        let mut index = HnswIndex::new(Similarity::Euclidean);

        let v1 = IndexedVector {
            id: 1,
            vector: vec![0.0, 0.0],
        };
        let v2 = IndexedVector {
            id: 2,
            vector: vec![1.0, 0.0],
        };

        index.insert(v1).unwrap();
        index.insert(v2).unwrap();

        assert!(index.index.nodes.contains_key(&1));
        assert!(index.index.nodes.contains_key(&2));

        // There should be a base layer (level 0)
        let n1 = index.index.nodes.get(&1).unwrap();
        let n2 = index.index.nodes.get(&2).unwrap();
        assert!(!n1.neighbors.is_empty());
        assert!(!n2.neighbors.is_empty());

        // At level 0, each should have at least one neighbor; commonly connected to each other
        let nbrs1_lvl0 = &n1.neighbors[0];
        let nbrs2_lvl0 = &n2.neighbors[0];
        assert!(!nbrs1_lvl0.is_empty());
        assert!(!nbrs2_lvl0.is_empty());

        // This check is lenient(probabilistic): only asserts that at least one side linked to the other
        let linked = nbrs1_lvl0.contains(&2) || nbrs2_lvl0.contains(&1);
        assert!(linked);
    }

    #[test]
    fn test_search_matches_flat_small() {
        let mut flat = FlatIndex::new();
        let mut hnsw = HnswIndex::new(Similarity::Euclidean);

        let data = vec![
            IndexedVector {
                id: 1,
                vector: vec![1.0, 0.0],
            },
            IndexedVector {
                id: 2,
                vector: vec![0.0, 1.0],
            },
            IndexedVector {
                id: 3,
                vector: vec![1.0, 1.0],
            },
            IndexedVector {
                id: 4,
                vector: vec![0.9, 0.1],
            },
            IndexedVector {
                id: 5,
                vector: vec![0.2, 0.8],
            },
        ];

        for v in data.clone() {
            flat.insert(v.clone()).unwrap();
            hnsw.insert(v).unwrap();
        }

        let queries = vec![vec![1.0, 0.2], vec![0.1, 0.9]];
        let k = 2;

        for q in queries {
            let flat_ids = flat.search(q.clone(), Similarity::Euclidean, k).unwrap();
            let hnsw_ids = hnsw.search(q.clone(), Similarity::Euclidean, k).unwrap();

            //both return the same number of results and that HNSW matches Flat for this tiny dataset
            assert_eq!(hnsw_ids.len(), k.min(flat_ids.len()));
            assert_eq!(hnsw_ids, flat_ids);
        }
    }

    #[test]
    fn test_search_empty_index_returns_empty() {
        let index = HnswIndex::new(Similarity::Euclidean);
        let res = index
            .search(vec![0.0, 0.0], Similarity::Euclidean, 3)
            .unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn test_search_k_zero_returns_empty() {
        let mut index = HnswIndex::new(Similarity::Euclidean);
        index
            .insert(IndexedVector {
                id: 1,
                vector: vec![0.0, 0.0],
            })
            .unwrap();
        let res = index
            .search(vec![0.0, 0.0], Similarity::Euclidean, 0)
            .unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn test_search_cosine_normalization_basic() {
        let mut index = HnswIndex::new(Similarity::Cosine);
        index
            .insert(IndexedVector {
                id: 1,
                vector: vec![1.0, 0.0],
            })
            .unwrap();
        index
            .insert(IndexedVector {
                id: 2,
                vector: vec![0.0, 1.0],
            })
            .unwrap();
        let res = index
            .search(vec![10.0, 0.0], Similarity::Cosine, 1)
            .unwrap();
        assert_eq!(res, vec![1]);
    }

    #[test]
    fn test_soft_delete_and_search_skip() {
        let mut index = HnswIndex::new(Similarity::Euclidean);
        index
            .insert(IndexedVector {
                id: 1,
                vector: vec![0.0, 0.0],
            })
            .unwrap();
        index
            .insert(IndexedVector {
                id: 2,
                vector: vec![1.0, 0.0],
            })
            .unwrap();
        index
            .insert(IndexedVector {
                id: 3,
                vector: vec![0.0, 1.0],
            })
            .unwrap();

        let existed = index.delete(2).unwrap();
        assert!(existed);
        let n2 = index.index.nodes.get(&2).expect("node 2 must exist");
        assert!(n2.deleted);

        // Search near id=2 should not return 2
        let res = index
            .search(vec![0.9, 0.1], Similarity::Euclidean, 2)
            .unwrap();
        assert!(!res.contains(&2));

        // Deleting a non-existent id returns false
        let existed = index.delete(999).unwrap();
        assert!(!existed);

        // If entry point was 2, it should be updated to a non-deleted id
        if let Some(ep) = index.index.entry_point {
            if ep == 2 {
                panic!("entry point should have been moved off deleted id");
            }
        }
    }

    #[test]
    fn test_stats_and_deleted_ratio() {
        let mut index = HnswIndex::new(Similarity::Euclidean);
        index
            .insert(IndexedVector {
                id: 1,
                vector: vec![0.0, 0.0],
            })
            .unwrap();
        index
            .insert(IndexedVector {
                id: 2,
                vector: vec![1.0, 0.0],
            })
            .unwrap();
        index
            .insert(IndexedVector {
                id: 3,
                vector: vec![0.0, 1.0],
            })
            .unwrap();
        index
            .insert(IndexedVector {
                id: 4,
                vector: vec![1.0, 1.0],
            })
            .unwrap();

        index.delete(2).unwrap();

        let stats = index.stats();
        assert_eq!(stats.alive + stats.deleted, index.index.nodes.len());
        assert_eq!(stats.deleted, 1);
        assert_eq!(stats.alive, index.index.nodes.len() - 1);

        // Ratio should be > 0 and <= 0.5 for 1/4 deleted
        let ratio = index.deleted_ratio();
        assert!(ratio > 0.0 && ratio <= 0.5, "ratio was {ratio}");

        // Histogram sums to alive count
        let sum_hist: usize = stats.level_histogram.values().sum();
        assert_eq!(sum_hist, stats.alive);
    }
}
