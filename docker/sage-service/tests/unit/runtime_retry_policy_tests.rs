//! Unit tests for `runtime/retry_policy.rs`.

use sage_service::runtime::retry_policy::*;

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
