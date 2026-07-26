//! Unit tests for `resilience.rs`.

use quench_starter::resilience::*;

#[test]
fn test_format_error_timeout() {
    let msg = format_error_message("Request timeout", "web_search");
    assert!(msg.contains("timed out"));
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
                    Err::<&str, &str>("Failed")
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
