//! Unit tests for `runtime/rate_limiter.rs`.

use sage_service::runtime::rate_limiter::*;

#[test]
fn test_rate_limit_check() {
    let limiter = RateLimiter::new();

    let result = limiter.check_rate_limit("web_assistant", Some("user-1"), Some("conv-1"));
    assert!(result.is_ok());
}

#[test]
fn test_burst_limit_exceeded() {
    let limiter = RateLimiter::new();

    // web_assistant has burst limit of 30
    for _ in 0..30 {
        limiter
            .check_rate_limit("web_assistant", Some("user-1"), Some("conv-1"))
            .unwrap();
    }

    // 31st call should fail
    let result = limiter.check_rate_limit("web_assistant", Some("user-1"), Some("conv-1"));
    assert!(result.is_err());
}

#[test]
fn test_remaining_calls() {
    let limiter = RateLimiter::new();

    // web_assistant: 120 calls/min
    for _ in 0..5 {
        limiter
            .check_rate_limit("web_assistant", Some("user-1"), None)
            .unwrap();
    }

    let remaining = limiter.get_user_remaining_calls("web_assistant", "user-1");
    assert_eq!(remaining, 115);
}

#[test]
fn test_reset_user() {
    let limiter = RateLimiter::new();

    limiter
        .check_rate_limit("web_assistant", Some("user-1"), None)
        .unwrap();

    let remaining_before = limiter.get_user_remaining_calls("web_assistant", "user-1");
    assert_eq!(remaining_before, 119);

    limiter.reset_user("user-1");

    let remaining_after = limiter.get_user_remaining_calls("web_assistant", "user-1");
    assert_eq!(remaining_after, 120);
}
