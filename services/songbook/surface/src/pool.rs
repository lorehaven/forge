//! A few threads for the calls to the service, so that a slow one holds up neither the screen nor the others.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};

type Job = Box<dyn FnOnce() + Send>;

/// The threads, and the way to give them work.
#[derive(Debug, Clone)]
pub struct Pool {
    jobs: Sender<Job>,
}

impl Pool {
    #[must_use]
    pub fn new(threads: usize) -> Self {
        let (jobs, rx): (Sender<Job>, Receiver<Job>) = channel();
        let rx = Arc::new(Mutex::new(rx));
        for _ in 0..threads.max(1) {
            let rx = Arc::clone(&rx);
            std::thread::spawn(move || {
                loop {
                    // The lock is held only while waiting for the next job, not while it runs.
                    let job = rx.lock().unwrap_or_else(PoisonError::into_inner).recv();
                    match job {
                        Ok(job) => job(),
                        Err(_) => return,
                    }
                }
            });
        }
        Self { jobs }
    }

    /// Runs `job` on one of the threads.
    pub fn run(&self, job: impl FnOnce() + Send + 'static) {
        let _ = self.jobs.send(Box::new(job));
    }
}
