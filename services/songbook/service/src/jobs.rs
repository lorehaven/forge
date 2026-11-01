//! The one job: sync, find links, download, convert... run in the background while the UI polls its log.

use songbook_core::Job;
use std::sync::{Arc, Mutex, PoisonError};

/// The log keeps at most this many bytes; older lines are dropped (offsets stay absolute).
const MAX_LOG: usize = 4 * 1024 * 1024;

#[derive(Debug, Default)]
struct State {
    name: Option<String>,
    running: bool,
    error: Option<String>,
    log: String,
    /// Absolute offset of `log`'s first byte (it moves when the front is dropped).
    base: u64,
}

/// Shared handle to the single job slot.
#[derive(Debug, Clone, Default)]
pub struct Jobs {
    state: Arc<Mutex<State>>,
}

/// What a running job writes its progress to.
#[derive(Debug, Clone)]
pub struct Log {
    state: Arc<Mutex<State>>,
}

impl Log {
    /// Appends a line (and logs it to the service log).
    pub fn line(&self, text: impl AsRef<str>) {
        let text = text.as_ref();
        quench_log::info!("job: {text}");
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        s.log.push_str(text);
        s.log.push('\n');
        if s.log.len() > MAX_LOG {
            let mut cut = s.log.len() - MAX_LOG / 2;
            while !s.log.is_char_boundary(cut) {
                cut += 1;
            }
            s.log.drain(..cut);
            s.base += cut as u64;
        }
    }
}

impl Jobs {
    #[must_use]
    pub fn running(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .running
    }

    /// Starts `name` unless a job is running. `work` gets the log and returns `Err(reason)` for a failed job.
    /// Returns whether it started.
    pub fn start<F, Fut>(&self, name: &str, work: F) -> bool
    where
        F: FnOnce(Log) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        {
            let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if s.running {
                return false;
            }
            s.name = Some(name.to_string());
            s.running = true;
            s.error = None;
            s.log.clear();
            s.base = 0;
        }
        let log = Log {
            state: Arc::clone(&self.state),
        };
        let state = Arc::clone(&self.state);
        tokio::spawn(async move {
            let outcome = work(log.clone()).await;
            if let Err(reason) = &outcome {
                log.line(format!("failed: {reason}"));
            }
            let mut s = state.lock().unwrap_or_else(PoisonError::into_inner);
            s.error = outcome.err();
            s.running = false;
        });
        true
    }

    /// The job and its log from absolute offset `from`. A `from` past the end (the client saw an older job) starts over.
    #[must_use]
    pub fn read(&self, from: u64) -> Job {
        let s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let end = s.base + s.log.len() as u64;
        let from = if from > end { 0 } else { from };
        let mut at = usize::try_from(from.saturating_sub(s.base))
            .unwrap_or(0)
            .min(s.log.len());
        while !s.log.is_char_boundary(at) {
            at += 1;
        }
        Job {
            name: s.name.clone(),
            running: s.running,
            error: s.error.clone(),
            log: s.log[at..].to_string(),
            offset: end,
        }
    }
}
