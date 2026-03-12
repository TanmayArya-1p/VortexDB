use defs::{DbError, SnapshottableDb};
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};
mod worker;
use crate::{metadata::Metadata, registry::SnapshotRegistry};

pub struct SnapshotEngine {
    last_k: usize, // only retain the last k snapshots on disk. old/stale snapshots are marked as dead on the registry
    snapshot_queue: Arc<Mutex<VecDeque<Metadata>>>,
    db: Arc<Mutex<dyn SnapshottableDb>>,
    registry: Arc<Mutex<dyn SnapshotRegistry>>,
    worker_ctx: WorkerContext,
}

pub struct WorkerContext {
    worker_cv: Arc<Condvar>,
    worker_running: Arc<Mutex<bool>>,
    join_handle: Option<JoinHandle<Result<(), DbError>>>,
}

impl WorkerContext {
    fn new() -> WorkerContext {
        WorkerContext {
            worker_cv: Arc::new(Condvar::new()),
            worker_running: Arc::new(Mutex::new(false)),
            join_handle: None,
        }
    }
}

impl SnapshotEngine {
    pub fn new(
        last_k: usize,
        db: Arc<Mutex<dyn SnapshottableDb>>,
        registry: Arc<Mutex<dyn SnapshotRegistry>>,
    ) -> Self {
        Self {
            last_k,
            snapshot_queue: Arc::new(Mutex::new(VecDeque::new())),
            db,
            registry,
            worker_ctx: WorkerContext::new(),
        }
    }

    // take a snapshot on the callers thread
    pub fn snapshot(&mut self) -> Result<(), DbError> {
        Self::take_snapshot(
            &mut self.db,
            &mut self.registry,
            &mut self.snapshot_queue,
            self.last_k,
        )
    }

    pub fn list_alive_snapshots(&mut self) -> Result<Vec<Metadata>, DbError> {
        Ok(self
            .snapshot_queue
            .lock()
            .map_err(|_| DbError::LockError)?
            .iter()
            .cloned()
            .collect())
    }

    // helper function to take snapshot
    fn take_snapshot(
        db: &mut Arc<Mutex<dyn SnapshottableDb>>,
        registry: &mut Arc<Mutex<dyn SnapshotRegistry>>,
        snapshot_queue: &mut Arc<Mutex<VecDeque<Metadata>>>,
        last_k: usize,
    ) -> Result<(), DbError> {
        let snapshot_path = db
            .lock()
            .map_err(|_| DbError::LockError)?
            .create_snapshot(
                registry
                    .lock()
                    .map_err(|_| DbError::LockError)?
                    .dir()
                    .as_path(),
            )
            .map_err(|err| {
                DbError::SnapshotEngineError(format!("Could not create snapshot : {}", err))
            })?;
        let snapshot_metadata = Metadata::parse(&snapshot_path).map_err(|err| {
            DbError::SnapshotEngineError(format!("Could not parse snapshot metadata: {}", err))
        })?;

        // add the snapshot to registry
        registry
            .lock()
            .map_err(|_| DbError::LockError)?
            .add_snapshot(&snapshot_path)
            .map_err(|err| {
                DbError::SnapshotEngineError(format!("Could not add snapshot to registry: {}", err))
            })?;

        {
            let mut queue = snapshot_queue.lock().map_err(|_| DbError::LockError)?;
            queue.push_back(snapshot_metadata);

            while queue.len() > last_k {
                let old = queue.pop_front().ok_or_else(|| {
                    DbError::SnapshotEngineError("Snapshot metadata queue is empty".to_string())
                })?;
                registry
                    .lock()
                    .map_err(|_| DbError::LockError)?
                    .mark_dead(old.small_id)
                    .map_err(|err| {
                        DbError::SnapshotEngineError(format!(
                            "Could not mark snapshot as dead in registry: {}",
                            err
                        ))
                    })?;
            }
            // drop queue lock
        }
        Ok(())
    }
}
