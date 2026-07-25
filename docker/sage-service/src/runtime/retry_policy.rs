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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_creation() {
        let policy = RetryPolicy::new(3, 100, 5000);
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.initial_backoff_ms, 100);
        assert_eq!(policy.max_backoff_ms, 5000);
    }

    #[test]
    fn test_exponential_backoff() {
        let policy = RetryPolicy::new(5, 100, 5000);

        let backoff0 = policy.calculate_backoff(0);
        assert_eq!(backoff0.as_millis(), 0);

        let backoff1 = policy.calculate_backoff(1);
        assert_eq!(backoff1.as_millis(), 100);

        let backoff2 = policy.calculate_backoff(2);
        assert_eq!(backoff2.as_millis(), 200);

        let backoff3 = policy.calculate_backoff(3);
        assert_eq!(backoff3.as_millis(), 400);

        // Should cap at max_backoff_ms
        let backoff_large = policy.calculate_backoff(20);
        assert_eq!(backoff_large.as_millis(), 5000);
    }

    #[test]
    fn test_should_retry() {
        let policy = RetryPolicy::new(3, 100, 1000);

        assert!(policy.should_retry(0));
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
    }

    #[test]
    fn test_tool_policies() {
        let web_search = get_retry_policy("web_search");
        assert_eq!(web_search.max_retries, 3);

        let command = get_retry_policy("command");
        assert_eq!(command.max_retries, 0);

        let code = get_retry_policy("code_executor");
        assert_eq!(code.max_retries, 1);
    }
}
