use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use defs::{DbError, SnapshottableDb};

use crate::{engine::SnapshotEngine, metadata::Metadata, registry::SnapshotRegistry};

impl SnapshotEngine {
    pub fn stop_worker(&mut self) -> Result<(), DbError> {
        {
            let mut worker_running = self
                .worker_ctx
                .worker_running
                .lock()
                .map_err(|_| DbError::LockError)?;
            if !*worker_running {
                return Err(DbError::SnapshotEngineError(
                    "Worker thread not running".to_string(),
                ));
            }
            *worker_running = false;
        }
        self.worker_ctx.worker_cv.notify_one();

        if let Some(handle) = self.worker_ctx.join_handle.take() {
            handle
                .join()
                .map_err(|_| {
                    DbError::SnapshotEngineError("Could not join worker thread".to_string())
                })?
                .map_err(|e| {
                    DbError::SnapshotEngineError(format!("Worker thread errored: {}", e))
                })?;
        }
        Ok(())
    }

    pub fn is_worker_alive(&self) -> Result<bool, DbError> {
        {
            let worker_running = self
                .worker_ctx
                .worker_running
                .lock()
                .map_err(|_| DbError::LockError)?;
            if !*worker_running {
                return Ok(false);
            }
        }

        match &self.worker_ctx.join_handle {
            Some(handle) => Ok(!handle.is_finished()),
            None => Ok(false),
        }
    }

    // notify the worker thread to take a snapshot now
    pub fn worker_snapshot(&mut self) -> Result<(), DbError> {
        // acquire lock for worker_running
        let worker_running = self
            .worker_ctx
            .worker_running
            .lock()
            .map_err(|_| DbError::LockError)?;
        if !*worker_running {
            return Err(DbError::SnapshotEngineError(
                "Worker thread not running".to_string(),
            ));
        }
        self.worker_ctx.worker_cv.notify_one();
        Ok(())
    }

    pub fn start_worker(&mut self, interval: i64) -> Result<(), DbError> {
        // acquire lock for worker_running
        {
            let mut worker_running = self
                .worker_ctx
                .worker_running
                .lock()
                .map_err(|_| DbError::LockError)?;
            if *worker_running {
                return Err(DbError::SnapshotEngineError(
                    "Worker thread already running".to_string(),
                ));
            }
            *worker_running = true;
        }

        let worker_running_clone = Arc::clone(&self.worker_ctx.worker_running);
        let db_clone = Arc::clone(&self.db);
        let registry_clone = Arc::clone(&self.registry);
        let worker_cv_clone = Arc::clone(&self.worker_ctx.worker_cv);
        let snapshot_queue_clone = Arc::clone(&self.snapshot_queue);
        let last_k_clone = self.last_k;

        let dur_interval = Duration::from_secs(interval as u64);
        self.worker_ctx.join_handle = Some(std::thread::spawn(move || -> Result<(), DbError> {
            Self::worker(
                dur_interval,
                last_k_clone,
                worker_running_clone,
                db_clone,
                registry_clone,
                worker_cv_clone,
                snapshot_queue_clone,
            )
        }));
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
    ) -> Result<(), DbError> {
        loop {
            {
                // acquire the lock and exit if its false
                let worker_running = worker_running.lock().map_err(|_| DbError::LockError)?;
                if !*worker_running {
                    break;
                }
            }

            Self::take_snapshot(&mut db, &mut registry, &mut snapshot_queue, last_k)?;

            let worker_running = worker_running.lock().map_err(|_| DbError::LockError)?;
            let _ = worker_cv
                .wait_timeout(worker_running, interval)
                .map_err(|err| {
                    DbError::SnapshotEngineError(format!("Failed to wait on worker cv : {}", err))
                })?;
        }
        Ok(())
    }
}
