//! Unit tests for `routers/ui/context_builder.rs`.

use sage_service::routers::ui::context_builder::*;

#[test]
fn test_token_usage_display() {
    let usage = TokenUsageInfo {
        total_messages: 5,
        total_tokens: 800,
        max_tokens: 2048,
        system_tokens: 200,
        near_limit: false,
        at_limit: false,
        utilization_percent: 39.1,
    };

    let display = usage.format_display();
    assert!(display.contains("800/2048"));
    assert!(display.contains("39.1%"));
    assert!(display.contains("Messages: 5"));
}

#[test]
fn test_warning_at_limit() {
    let usage = TokenUsageInfo {
        total_messages: 10,
        total_tokens: 2000,
        max_tokens: 2048,
        system_tokens: 200,
        near_limit: false,
        at_limit: true,
        utilization_percent: 97.7,
    };

    assert!(usage.warning_message().is_some());
    assert!(usage.warning_message().unwrap().contains("at capacity"));
}

#[test]
fn test_warning_near_limit() {
    let usage = TokenUsageInfo {
        total_messages: 8,
        total_tokens: 1638,
        max_tokens: 2048,
        system_tokens: 200,
        near_limit: true,
        at_limit: false,
        utilization_percent: 80.0,
    };

    assert!(usage.warning_message().is_some());
    assert!(usage.warning_message().unwrap().contains("80%"));
}

#[test]
fn test_no_warning_under_limit() {
    let usage = TokenUsageInfo {
        total_messages: 3,
        total_tokens: 400,
        max_tokens: 2048,
        system_tokens: 200,
        near_limit: false,
        at_limit: false,
        utilization_percent: 19.5,
    };

    assert!(usage.warning_message().is_none());
}
