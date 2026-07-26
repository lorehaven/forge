//! Unit tests for `domain/context.rs`.

use sage_service::domain::context::*;

#[test]
fn test_context_limits() {
    let limits = get_context_limits("web_assistant");
    assert_eq!(limits.max_tokens, 4096);
    assert_eq!(limits.warning_threshold_percent, 80);
}

#[test]
fn test_token_counting() {
    // ~1 token per 4 characters
    let tokens = TokenCounter::count_tokens("hello world"); // 11 chars -> ~3 tokens
    assert!((2..=4).contains(&tokens));
}

#[test]
fn test_context_status() {
    let status = ContextStatus::new("web_assistant", 3280);
    assert_eq!(status.max_tokens, 4096);
    assert_eq!(status.remaining_tokens, 816);
    assert!(status.is_near_limit);
    assert!(!status.is_at_limit);
    assert!((status.usage_percent - 80.0).abs() < 1.0);
}

#[test]
fn test_context_warning() {
    // Near limit
    let warning = ContextManager::check_context("web_assistant", 3500);
    assert!(warning.is_some());
    assert_eq!(warning.unwrap().warning_type, ContextWarningType::NearLimit);

    // At limit
    let warning = ContextManager::check_context("web_assistant", 4096);
    assert!(warning.is_some());
    assert_eq!(warning.unwrap().warning_type, ContextWarningType::AtLimit);

    // Safe
    let warning = ContextManager::check_context("web_assistant", 2000);
    assert!(warning.is_none());
}

#[test]
fn test_prune_messages() {
    let mut messages = vec![
        ("system".to_string(), "You are helpful".to_string()),
        ("user".to_string(), "Hello there".to_string()),
        ("assistant".to_string(), "Hi! How can I help?".to_string()),
        ("user".to_string(), "Tell me a joke".to_string()),
    ];

    let freed = ContextManager::prune_messages(&mut messages, 50);
    assert!(freed > 0);
    assert_eq!(messages[0].0, "system"); // System message preserved
    assert!(messages.len() < 4); // Some messages removed
}
