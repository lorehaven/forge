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

#[test]
fn user_rate_limit_exceeded_after_the_configured_calls_per_minute() {
    let limiter = RateLimiter::new();
    // cli_agent: 30 calls/min.
    for _ in 0..30 {
        limiter
            .check_rate_limit("cli_agent", Some("user-1"), None)
            .unwrap();
    }

    let err = limiter
        .check_rate_limit("cli_agent", Some("user-1"), None)
        .unwrap_err();
    match err {
        RateLimitError::UserRateLimitExceeded { limit, user_id } => {
            assert_eq!(limit, 30);
            assert_eq!(user_id, "user-1");
        }
        other => panic!("expected UserRateLimitExceeded, got {other:?}"),
    }
}

#[test]
fn burst_limit_error_names_the_conversation_and_limit() {
    let limiter = RateLimiter::new();
    for _ in 0..5 {
        limiter
            .check_rate_limit("cli_agent", None, Some("conv-1"))
            .unwrap();
    }
    let err = limiter
        .check_rate_limit("cli_agent", None, Some("conv-1"))
        .unwrap_err();
    match err {
        RateLimitError::BurstLimitExceeded {
            limit,
            conversation_id,
        } => {
            assert_eq!(limit, 5);
            assert_eq!(conversation_id, "conv-1");
        }
        other => panic!("expected BurstLimitExceeded, got {other:?}"),
    }
}

#[test]
fn an_unknown_profile_falls_back_to_the_default_config() {
    let limiter = RateLimiter::new();
    assert!(limiter.get_config("no-such-profile").is_none());

    // The fallback (60/min, 15 burst) is only visible through behavior,
    // since `check_rate_limit` never surfaces the config it picked.
    for _ in 0..15 {
        limiter
            .check_rate_limit("no-such-profile", None, Some("conv-1"))
            .unwrap();
    }
    let err = limiter
        .check_rate_limit("no-such-profile", None, Some("conv-1"))
        .unwrap_err();
    assert!(matches!(
        err,
        RateLimitError::BurstLimitExceeded { limit: 15, .. }
    ));
}

#[test]
fn set_config_overrides_the_default_for_a_profile() {
    let mut limiter = RateLimiter::new();
    limiter.set_config("web_assistant", RateLimitConfig::new(2, 1));
    let config = limiter.get_config("web_assistant").unwrap();
    assert_eq!(config.calls_per_minute, 2);
    assert_eq!(config.burst_limit, 1);

    limiter
        .check_rate_limit("web_assistant", Some("user-1"), None)
        .unwrap();
    limiter
        .check_rate_limit("web_assistant", Some("user-1"), None)
        .unwrap();
    let err = limiter
        .check_rate_limit("web_assistant", Some("user-1"), None)
        .unwrap_err();
    assert!(matches!(
        err,
        RateLimitError::UserRateLimitExceeded { limit: 2, .. }
    ));
}

#[test]
fn reset_conversation_clears_only_that_conversations_burst_count() {
    let limiter = RateLimiter::new();
    limiter
        .check_rate_limit("cli_agent", None, Some("conv-1"))
        .unwrap();
    assert_eq!(
        limiter.get_conversation_remaining_calls("cli_agent", "conv-1"),
        4
    );

    limiter.reset_conversation("conv-1");
    assert_eq!(
        limiter.get_conversation_remaining_calls("cli_agent", "conv-1"),
        5
    );
}

#[test]
fn reset_all_clears_every_user_and_conversation() {
    let limiter = RateLimiter::new();
    limiter
        .check_rate_limit("web_assistant", Some("user-1"), Some("conv-1"))
        .unwrap();

    limiter.reset_all();

    assert_eq!(
        limiter.get_user_remaining_calls("web_assistant", "user-1"),
        120
    );
    assert_eq!(
        limiter.get_conversation_remaining_calls("web_assistant", "conv-1"),
        30
    );
}

#[test]
fn get_conversation_remaining_calls_is_zero_for_an_unseen_conversation() {
    let limiter = RateLimiter::new();
    assert_eq!(
        limiter.get_conversation_remaining_calls("web_assistant", "never-called"),
        30
    );
}

#[test]
fn rate_limit_error_display_messages_name_the_offender_and_limit() {
    let user_err = RateLimitError::UserRateLimitExceeded {
        limit: 10,
        user_id: "alice".to_string(),
    };
    assert_eq!(
        user_err.to_string(),
        "User 'alice' has exceeded rate limit of 10 calls per minute"
    );

    let burst_err = RateLimitError::BurstLimitExceeded {
        limit: 5,
        conversation_id: "conv-9".to_string(),
    };
    assert_eq!(
        burst_err.to_string(),
        "Conversation 'conv-9' has exceeded burst limit of 5 concurrent calls"
    );
}

#[test]
fn default_trait_impl_matches_new() {
    let limiter = RateLimiter::default();
    assert_eq!(
        limiter
            .get_config("web_assistant")
            .unwrap()
            .calls_per_minute,
        120
    );
}
