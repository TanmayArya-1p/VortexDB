# Snapshots

To snapshot the database, we need to primarily snapshot 2 things:

- Index snapshot : Record all the nodes and edges, so rebuilding the graph is simple
- Storage snapshot : Record the `Payload` for all `pointID`s

Additionally, we also require some metadata to restore the database completely. Here is the struct that describes a self-contained snapshot:
```rust
pub struct Snapshot {
    pub id: Uuid, // used for uniqueling identifying snapshots
    pub date: SystemTime, // when it was created
    pub sem_ver: Version, // version of snapshotting system. This is used so we don't try to restore a snapshot created by an old version of the snapshot crate
    pub index_snapshot: IndexSnapshot,
    pub storage_snapshot: StorageCheckpoint,
    pub dimensions: usize, // dimension of vectors
}
```
## Index Snapshots

Each Index that supports snapshotting must satisfy the following trait:

```rust
pub trait SerializableIndex {
    fn serialize_topology(&self) -> Result<Vec<u8>, DbError>; // stream of bytes indicating topology of graph
    fn serialize_metadata(&self) -> Result<Vec<u8>, DbError>; // stream of bytes that contain index-specific metadata needed for restore

    fn snapshot(&self) -> Result<IndexSnapshot, DbError>;

    // topology simple contains a relationship between PointIDs, so this method is required to populate the index cache with vectors for PointIDs
    fn populate_vectors(&mut self, storage: &dyn StorageEngine) -> Result<(), DbError>;  PointIDs
}
```
The implementation of `serialize_topology` and `serialize_metadata` depends on the indexer itself, but the important method here is the `snapshot` method. The `snapshot` method effectively calls `serialize_metadata` and `serialize_topology` and returns an `IndexSnapshot` described below:

```rust
pub struct IndexSnapshot {
    pub index_type: IndexType,
    pub magic: Magic, // magic bytes associated with indexer
    pub topology_b: Vec<u8>, // from serialize_topology
    pub metadata_b: Vec<u8>, // from serialize_metadata
}
```
Each index also has a `deserialize` function that takes in an `IndexSnapshot` and restores the index. Once the index has been restored, it has to be populated with vectors using the `populate_vectors` function of the `SerializableIndex` trait.

## Storage Snapshots

The `StorageEngine` trait additionally has 2 methods:
```rust
    fn checkpoint_at(&self, path: &Path) -> Result<checkpoint::StorageCheckpoint>;
    fn restore_checkpoint(&mut self, checkpoint: &checkpoint::StorageCheckpoint) -> Result<()>;
```
In RocksDB, this uses the RocksDB checkpoint API in its implementation.
The `StorageCheckpoint` struct looks like this:

```rust
pub struct StorageCheckpoint {
    pub path: PathBuf,
    pub storage_type: StorageType,
}
```

## Saving a Snapshot

Finally, after getting a `StorageCheckpoint` and `IndexSnapshot`, a new `Snapshot` can be created by calling the following function:
```rust
impl Snapshot {
    pub fn new(
        index_snapshot: IndexSnapshot,
        storage_snapshot: StorageCheckpoint,
        dimensions: usize,
    ) -> Result<Snapshot, DbError> { ... }
}
```
The Snapshot object can be saved using the `save` function that creates a tarball with the following things inside it:

- `kdtree-index-meta.bin` : contains Index metadata bytes
- `kdtree-index-topo.bin` : contains Index topology bytes
- Storage checkpoint file
- `manifest.json` - Contains checksums for the snapshot files that are verified during restore, and also other metadata required for restore.

## Snapshot Engine

The `SnapshotEngine` manages snapshots by spawning a worker thread that takes a snapshot of the database at regular intervals of time. It cycles through `k` latest snapshots by deleting the oldest and append the newest snapshot. The `SnapshotEngine` uses a `SnapshotRegistry` to store snapshots. The `SnapshotRegistry` is a trait described below:

```rust
pub trait SnapshotRegistry: Send + Sync {
    fn add_snapshot(&mut self, snapshot_path: &Path) -> Result<Metadata, DbError>;

    fn list_snapshots(&mut self, limit: usize, offset: usize) -> Result<SnapshotMetaPage, DbError>;
    fn get_latest_snapshot(&mut self) -> Result<Metadata, DbError>;

    fn get_metadata(&mut self, small_id: String) -> Result<Metadata, DbError>;
    fn remove_snapshot(&mut self, small_id: String) -> Result<Metadata, DbError>;

    fn load(
        &mut self,
        small_id: String,
        storage_data_path: &Path,
    ) -> Result<VectorDbRestore, DbError>;
    fn dir(&self) -> PathBuf;

    // in the future this could be used to maybe move an old/stale snapshot to cold storage or to a remote registry
    fn mark_dead(&mut self, small_id: String) -> Result<Metadata, DbError>; // current behaviour is to call remove_snapshot;
    fn list_alive_snapshots(&mut self) -> Result<SnapshotMetaPage, DbError>; // current behaviour is to call list_snapshots;
}
```
Currently, the only implementation of this trait is `LocalRegistry`, which stores snapshots locally and effectively deletes them when they are marked as dead.
