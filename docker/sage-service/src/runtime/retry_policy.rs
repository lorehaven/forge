use quench_serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl RetryPolicy {
    #[must_use]
    pub const fn new(max_retries: u32, initial_backoff_ms: u64, max_backoff_ms: u64) -> Self {
        Self {
            max_retries,
            initial_backoff_ms,
            max_backoff_ms,
        }
    }

    #[must_use]
    pub const fn none() -> Self {
        Self {
            max_retries: 0,
            initial_backoff_ms: 0,
            max_backoff_ms: 0,
        }
    }

    #[must_use]
    pub fn calculate_backoff(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::from_millis(0);
        }

        let backoff_ms = self.initial_backoff_ms * 2_u64.pow(attempt - 1);
        let backoff_ms = backoff_ms.min(self.max_backoff_ms);
        Duration::from_millis(backoff_ms)
    }

    #[must_use]
    pub const fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_retries
    }
}

#[must_use]
pub fn get_retry_policy(tool_name: &str) -> RetryPolicy {
    match tool_name {
        // Network-based tools: retry up to 3 times with exponential backoff
        "web_search" | "web_fetch" => RetryPolicy::new(3, 100, 5000),
        // Commands, file operations and calculator calls are not retried.
        "command" | "file_ops" | "calculator" => RetryPolicy::none(),
        // Code execution and unknown tools receive one retry.
        _ => RetryPolicy::new(1, 100, 1000),
    }
}
