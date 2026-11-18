//! Fixed-window counters in memory: how often one key may do one thing.
//!
//! Enough for one replica: a restart resets the counters, which only ever lets a few extra requests through.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct Limits {
    windows: Mutex<HashMap<String, (i64, u32)>>,
}

impl Limits {
    /// Counts one use of `key` at time `now` (Unix seconds). `Err(seconds)` says when the window ends, when `key` has already
    /// used up `max` in the current window of `window` seconds.
    ///
    /// # Errors
    ///
    /// The seconds until the key may try again.
    pub fn hit(&self, key: &str, max: u32, window: i64, now: i64) -> Result<(), u64> {
        let mut windows = self
            .windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if windows.len() > 10_000 {
            windows.retain(|_, (start, _)| now - *start < 3600);
        }
        let entry = windows.entry(key.to_string()).or_insert((now, 0));
        if now - entry.0 >= window {
            *entry = (now, 0);
        }
        if entry.1 >= max {
            return Err(u64::try_from(entry.0 + window - now).unwrap_or(1).max(1));
        }
        entry.1 += 1;
        Ok(())
    }

    /// Forgets a key (after a success that should not count).
    pub fn clear(&self, key: &str) {
        self.windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(key);
    }
}
