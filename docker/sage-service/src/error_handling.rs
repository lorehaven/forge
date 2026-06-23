use std::time::Duration;
use tokio::time::sleep;

/// Retry configuration
#[derive(Clone, Debug)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_multiplier: 2.0,
        }
    }
}

/// Retry a function with exponential backoff
pub async fn retry_with_backoff<F, Fut, T, E>(mut f: F, config: RetryConfig) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut delay = config.initial_delay_ms;
    let mut attempt = 1;

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                if attempt >= config.max_attempts {
                    return Err(err);
                }

                tracing::warn!(
                    "Attempt {} failed: {}. Retrying in {}ms...",
                    attempt,
                    err,
                    delay
                );

                sleep(Duration::from_millis(delay)).await;
                attempt += 1;

                // Calculate next delay with exponential backoff
                delay = ((delay as f64) * config.backoff_multiplier) as u64;
                delay = delay.min(config.max_delay_ms);
            }
        }
    }
}

/// User-friendly error messages
pub fn format_error_message(error: &str, tool_name: &str) -> String {
    if error.contains("timeout") || error.contains("time out") {
        format!(
            "The {} request timed out. The service may be busy or unreachable. Please try again.",
            tool_name
        )
    } else if error.contains("connection") || error.contains("connect") {
        format!(
            "Could not connect to the {} service. Please check your internet connection and try again.",
            tool_name
        )
    } else if error.contains("rate limit") || error.contains("429") {
        format!(
            "{} is rate limiting requests. Please wait a moment and try again.",
            tool_name
        )
    } else if error.contains("unauthorized") || error.contains("401") || error.contains("api key") {
        format!(
            "Authentication failed for {}. The API key may be invalid or expired.",
            tool_name
        )
    } else if error.contains("not found") || error.contains("404") {
        format!(
            "{} could not find what you're looking for. Try rephrasing your query.",
            tool_name
        )
    } else {
        format!("{} encountered an error: {}", tool_name, error)
    }
}

/// Circuit breaker for failing services
pub struct CircuitBreaker {
    failure_threshold: u32,
    success_threshold: u32,
    timeout_secs: u64,
    failure_count: std::sync::atomic::AtomicU32,
    success_count: std::sync::atomic::AtomicU32,
    last_failure_time: std::sync::Mutex<Option<std::time::SystemTime>>,
    state: std::sync::Mutex<CircuitState>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    Closed,   // Normal operation
    Open,     // Failing, reject requests
    HalfOpen, // Testing if service recovered
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, success_threshold: u32, timeout_secs: u64) -> Self {
        Self {
            failure_threshold,
            success_threshold,
            timeout_secs,
            failure_count: std::sync::atomic::AtomicU32::new(0),
            success_count: std::sync::atomic::AtomicU32::new(0),
            last_failure_time: std::sync::Mutex::new(None),
            state: std::sync::Mutex::new(CircuitState::Closed),
        }
    }

    pub fn call_succeeded(&self) {
        let mut state = self.state.lock().unwrap();

        match *state {
            CircuitState::HalfOpen => {
                // Service is recovering
                self.success_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let successes = self
                    .success_count
                    .load(std::sync::atomic::Ordering::Relaxed);

                if successes >= self.success_threshold {
                    *state = CircuitState::Closed;
                    self.failure_count
                        .store(0, std::sync::atomic::Ordering::Relaxed);
                    self.success_count
                        .store(0, std::sync::atomic::Ordering::Relaxed);
                    tracing::info!("Circuit breaker recovered - back to Closed state");
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count
                    .store(0, std::sync::atomic::Ordering::Relaxed);
            }
            _ => {}
        }
    }

    pub fn call_failed(&self) {
        let mut state = self.state.lock().unwrap();
        let mut last_failure = self.last_failure_time.lock().unwrap();

        self.failure_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *last_failure = Some(std::time::SystemTime::now());

        let failures = self
            .failure_count
            .load(std::sync::atomic::Ordering::Relaxed);

        if failures >= self.failure_threshold && *state != CircuitState::Open {
            *state = CircuitState::Open;
            tracing::warn!("Circuit breaker opened after {} failures", failures);
        }
    }

    pub fn is_available(&self) -> bool {
        let mut state = self.state.lock().unwrap();

        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has passed to try half-open
                if let Ok(last_failure) = self.last_failure_time.lock()
                    && let Some(failure_time) = *last_failure
                    && let Ok(elapsed) = failure_time.elapsed()
                    && elapsed.as_secs() >= self.timeout_secs
                {
                    *state = CircuitState::HalfOpen;
                    self.success_count
                        .store(0, std::sync::atomic::Ordering::Relaxed);
                    tracing::info!("Circuit breaker entering HalfOpen state");
                    return true;
                }
                false
            }
            CircuitState::HalfOpen => true, // Allow test calls
        }
    }

    pub fn get_state(&self) -> CircuitState {
        *self.state.lock().unwrap()
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, 3, 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_error_timeout() {
        let msg = format_error_message("Request timeout", "web_search");
        assert!(msg.contains("timed out"));
    }

    #[test]
    fn test_format_error_connection() {
        let msg = format_error_message("Connection refused", "web_fetch");
        assert!(msg.contains("connect"));
    }

    #[test]
    fn test_circuit_breaker_closed() {
        let cb = CircuitBreaker::new(3, 2, 5);
        assert!(cb.is_available());
        assert_eq!(cb.get_state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens() {
        let cb = CircuitBreaker::new(2, 1, 5);
        cb.call_failed();
        cb.call_failed();

        assert!(!cb.is_available());
        assert_eq!(cb.get_state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_recovers() {
        let cb = CircuitBreaker::new(2, 2, 5);
        cb.call_failed();
        cb.call_failed();

        // Now it's open, call_succeeded in half-open should work
        cb.call_succeeded();
        cb.call_succeeded();

        assert_eq!(cb.get_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_retry_success() {
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = call_count.clone();
        let result = retry_with_backoff(
            move || {
                let count = call_count_clone.clone();
                async move {
                    let current = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if current < 2 {
                        Err("Failed")
                    } else {
                        Ok("Success")
                    }
                }
            },
            RetryConfig::default(),
        )
        .await;

        assert_eq!(result, Ok("Success"));
        assert_eq!(call_count.load(std::sync::atomic::Ordering::Relaxed), 3);
    }
}
