use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl RetryPolicy {
    pub fn new(max_retries: u32, initial_backoff_ms: u64, max_backoff_ms: u64) -> Self {
        Self {
            max_retries,
            initial_backoff_ms,
            max_backoff_ms,
        }
    }

    pub fn none() -> Self {
        Self {
            max_retries: 0,
            initial_backoff_ms: 0,
            max_backoff_ms: 0,
        }
    }

    pub fn calculate_backoff(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::from_millis(0);
        }

        let backoff_ms = self.initial_backoff_ms * 2_u64.pow(attempt - 1);
        let backoff_ms = backoff_ms.min(self.max_backoff_ms);
        Duration::from_millis(backoff_ms)
    }

    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_retries
    }
}

pub fn get_retry_policy(tool_name: &str) -> RetryPolicy {
    match tool_name {
        // Network-based tools: retry up to 3 times with exponential backoff
        "web_search" => RetryPolicy::new(3, 100, 5000),
        "web_fetch" => RetryPolicy::new(3, 100, 5000),
        // Code execution: retry once (avoid side effects)
        "code_executor" => RetryPolicy::new(1, 100, 1000),
        // Commands: never retry (avoid side effects from shell execution)
        "command" => RetryPolicy::none(),
        // File operations: no retry (avoid partial writes)
        "file_ops" => RetryPolicy::none(),
        // Calculator: no retry (deterministic)
        "calculator" => RetryPolicy::none(),
        // Default: single retry
        _ => RetryPolicy::new(1, 100, 1000),
    }
}
