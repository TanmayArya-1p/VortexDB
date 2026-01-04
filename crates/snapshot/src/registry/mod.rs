// what do i need this to do?
// manage a source of snapshots and load and unload metadata
// load latest snapshot, load latest snapshot before x (ok but)
// manage a worker thread that is woken up at regular intervals or at the call of a function to take snapshot
// it accepts an arc
//
// broad architecture - what i want:
// - abstract snapshot source -> can be local directory or remote(define protocol)
// source operations:
//      - add snapshot (with Snapshot)
//      - read snapshot metadatas with paging
//      - read Snapshot of specific snapshot - internal implementation: unpack and read manifest file`(dont bother with checksums verification)
//             - make a proxy wrapper that deletes the temp file on destroy - caching is internal implementation
//

use std::path::{Path, PathBuf};

use defs::DbError;
pub mod constants;
pub mod local;
use crate::{Snapshot, VectorDbRestore, metadata::Metadata};

pub type SnapshotMetaPage = Vec<Metadata>;

pub const INFINITY_LIMIT: usize = 100000;
pub const NO_OFFSET: usize = 0;

pub trait SnapshotRegistry: Send + Sync {
    fn add_snapshot(&mut self, snapshot: &Snapshot) -> Result<(), DbError>;
    fn list_snapshots(&mut self, limit: usize, offset: usize) -> Result<SnapshotMetaPage, DbError>;
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
