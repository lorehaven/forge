use std::time::Duration;
use tokio::time::sleep;

/// Retry configuration for exponential backoff
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

/// Execute a function with exponential backoff retry
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

                delay = ((delay as f64) * config.backoff_multiplier) as u64;
                delay = delay.min(config.max_delay_ms);
            }
        }
    }
}

/// Format error messages for user-friendly output
pub fn format_error_message(error: &str, context: &str) -> String {
    let error_lower = error.to_lowercase();

    if error_lower.contains("timeout") || error_lower.contains("time out") {
        format!(
            "The {} request timed out. The service may be busy or unreachable. Please try again.",
            context
        )
    } else if error_lower.contains("connection") || error_lower.contains("connect") {
        format!(
            "Could not connect to {}. Please check your internet connection and try again.",
            context
        )
    } else if error_lower.contains("rate limit") || error_lower.contains("429") {
        format!(
            "{} is rate limiting requests. Please wait a moment and try again.",
            context
        )
    } else if error_lower.contains("unauthorized")
        || error_lower.contains("401")
        || error_lower.contains("api key")
    {
        format!(
            "Authentication failed for {}. The API key may be invalid or expired.",
            context
        )
    } else if error_lower.contains("not found") || error_lower.contains("404") {
        format!(
            "{} could not find what you're looking for. Try rephrasing your request.",
            context
        )
    } else {
        format!("{} encountered an error: {}", context, error)
    }
}

/// Circuit breaker state machine for failing services
pub struct CircuitBreaker {
    failure_threshold: u32,
    success_threshold: u32,
    timeout_secs: u64,
    failure_count: std::sync::atomic::AtomicU32,
    success_count: std::sync::atomic::AtomicU32,
    last_failure_time: std::sync::Mutex<Option<std::time::SystemTime>>,
    state: std::sync::Mutex<CircuitState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            CircuitState::HalfOpen => true,
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
