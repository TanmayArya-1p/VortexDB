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
    db: Arc<dyn SnapshottableDb>,
    registry: Arc<Mutex<dyn SnapshotRegistry>>,
    worker_cv: Arc<Condvar>,
    worker_running: Arc<Mutex<bool>>,
}
impl SnapshotEngine {
    pub fn new(
        interval: usize,
        last_k: usize,
        db: Arc<dyn SnapshottableDb>,
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

    pub fn snapshot(&mut self) -> Result<(), DbError> {
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

    // TODO: ask someone about sync issues (i dont think there are any)
    fn worker(
        interval: Duration,
        last_k: usize,
        worker_running: Arc<Mutex<bool>>,
        db: Arc<dyn SnapshottableDb>,
        registry: Arc<Mutex<dyn SnapshotRegistry>>,
        worker_cv: Arc<Condvar>,
        snapshot_queue: Arc<Mutex<VecDeque<Metadata>>>,
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

            let snapshot_path = db
                .create_snapshot(registry.lock().unwrap().dir().as_path())
                .unwrap();
            let snapshot_metadata = Metadata::parse(&snapshot_path).unwrap();

            {
                let mut queue = snapshot_queue.lock().unwrap();
                queue.push_back(snapshot_metadata);

                while queue.len() > last_k {
                    let old = queue.pop_front().unwrap();
                    registry.lock().unwrap().mark_dead(old.small_id).unwrap();
                }

                // drop queue lock
            }

            let _ = worker_cv.wait_timeout(worker_running, interval).unwrap();
        }
    }
}
