use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use defs::{DbError, SnapshottableDb};

use crate::{metadata::Metadata, registry::SnapshotRegistry};

pub struct SnapshotEngine {
    last_k: usize, // only retain the last k snapshots on disk. old/stale snapshots are marked as dead on the registry
    snapshot_queue: Arc<Mutex<VecDeque<Metadata>>>,
    db: Arc<Mutex<dyn SnapshottableDb>>,
    registry: Arc<Mutex<dyn SnapshotRegistry>>,
    worker_cv: Arc<Condvar>,
    worker_running: Arc<Mutex<bool>>,
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
            worker_cv: Arc::new(Condvar::new()),
            worker_running: Arc::new(Mutex::new(false)),
        }
    }

    pub fn stop_worker(&mut self) -> Result<(), DbError> {
        // acquire lock for worker_running
        let mut worker_running = self.worker_running.lock().map_err(|_| DbError::LockError)?;
        if !*worker_running {
            return Err(DbError::SnapshotEngineError(
                "Worker thread not running".to_string(),
            ));
        }
        *worker_running = false;
        self.worker_cv.notify_one();
        Ok(())
    }

    // notify the worker thread to take a snapshot now
    pub fn worker_snapshot(&mut self) -> Result<(), DbError> {
        // acquire lock for worker_running
        let worker_running = self.worker_running.lock().map_err(|_| DbError::LockError)?;
        if !*worker_running {
            return Err(DbError::SnapshotEngineError(
                "Worker thread not running".to_string(),
            ));
        }
        self.worker_cv.notify_one();
        Ok(())
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

    pub fn start_worker(&mut self, interval: i64) -> Result<(), DbError> {
        // acquire lock for worker_running
        let mut worker_running = self.worker_running.lock().map_err(|_| DbError::LockError)?;
        if *worker_running {
            return Err(DbError::SnapshotEngineError(
                "Worker thread already running".to_string(),
            ));
        }
        *worker_running = true;

        let worker_running_clone = Arc::clone(&self.worker_running);
        let db_clone = Arc::clone(&self.db);
        let registry_clone = Arc::clone(&self.registry);
        let worker_cv_clone = Arc::clone(&self.worker_cv);
        let snapshot_queue_clone = Arc::clone(&self.snapshot_queue);
        let last_k_clone = self.last_k;

        let dur_interval = Duration::from_secs(interval as u64);
        let _ = std::thread::spawn(move || {
            Self::worker(
                dur_interval,
                last_k_clone,
                worker_running_clone,
                db_clone,
                registry_clone,
                worker_cv_clone,
                snapshot_queue_clone,
            );
        });
        Ok(())
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

    // TODO: fix sync issues if any (i dont think there are any)
    fn worker(
        interval: Duration,
        last_k: usize,
        worker_running: Arc<Mutex<bool>>,
        mut db: Arc<Mutex<dyn SnapshottableDb>>,
        mut registry: Arc<Mutex<dyn SnapshotRegistry>>,
        worker_cv: Arc<Condvar>,
        mut snapshot_queue: Arc<Mutex<VecDeque<Metadata>>>,
    ) {
        loop {
            // acquire the lock and exit if its false
            let worker_running = worker_running.lock().unwrap();
            if !*worker_running {
                break;
            }

            Self::take_snapshot(&mut db, &mut registry, &mut snapshot_queue, last_k).unwrap();

            let _ = worker_cv.wait_timeout(worker_running, interval).unwrap();
        }
    }
}
