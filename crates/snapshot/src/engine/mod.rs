use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use defs::{DbError, SnapshottableDb};

use crate::{metadata::Metadata, registry::SnapshotRegistry};

pub struct SnapshotEngine {
    interval: Duration,
    last_k: usize,
    snapshot_queue: Arc<Mutex<VecDeque<Metadata>>>,
    db: Arc<Mutex<dyn SnapshottableDb>>,
    registry: Arc<Mutex<dyn SnapshotRegistry>>,
    worker_cv: Arc<Condvar>,
    worker_running: Arc<Mutex<bool>>,
}
impl SnapshotEngine {
    pub fn new(
        interval: usize,
        last_k: usize,
        db: Arc<Mutex<dyn SnapshottableDb>>,
        registry: Arc<Mutex<dyn SnapshotRegistry>>,
    ) -> Self {
        Self {
            interval: Duration::from_secs(interval as u64),
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
            return Err(DbError::StorageEngineError(
                "Worker thread not running".to_string(),
            ));
        }
        *worker_running = false;
        self.worker_cv.notify_one();
        Ok(())
    }

    // notify the worker to take a snapshot now
    pub fn worker_snapshot(&mut self) -> Result<(), DbError> {
        // acquire lock for worker_running
        let worker_running = self.worker_running.lock().map_err(|_| DbError::LockError)?;
        if !*worker_running {
            return Err(DbError::StorageEngineError(
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

    pub fn start_worker(&mut self) -> Result<(), DbError> {
        // acquire lock for worker_running
        let mut worker_running = self.worker_running.lock().map_err(|_| DbError::LockError)?;
        if *worker_running {
            return Err(DbError::StorageEngineError(
                "Worker thread already running".to_string(),
            ));
        }
        *worker_running = true;

        let worker_running_clone = Arc::clone(&self.worker_running);
        let db_clone = Arc::clone(&self.db);
        let registry_clone = Arc::clone(&self.registry);
        let worker_cv_clone = Arc::clone(&self.worker_cv);
        let snapshot_queue_clone = Arc::clone(&self.snapshot_queue);
        let interval_clone = self.interval;
        let last_k_clone = self.last_k;

        let _ = std::thread::spawn(move || {
            Self::worker(
                interval_clone,
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
            .unwrap()
            .create_snapshot(registry.lock().unwrap().dir().as_path())
            .unwrap();
        let snapshot_metadata = Metadata::parse(&snapshot_path).unwrap();

        // add the snapshot to registry
        registry
            .lock()
            .unwrap()
            .add_snapshot(&snapshot_path)
            .unwrap();

        {
            let mut queue = snapshot_queue.lock().unwrap();
            queue.push_back(snapshot_metadata);

            while queue.len() > last_k {
                let old = queue.pop_front().unwrap();
                registry.lock().unwrap().mark_dead(old.small_id).unwrap();
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
            let worker_running = worker_running
                .lock()
                .map_err(|_| DbError::LockError)
                .unwrap();
            if !*worker_running {
                break;
            }

            Self::take_snapshot(&mut db, &mut registry, &mut snapshot_queue, last_k).unwrap();

            let _ = worker_cv.wait_timeout(worker_running, interval).unwrap();
        }
    }
}
