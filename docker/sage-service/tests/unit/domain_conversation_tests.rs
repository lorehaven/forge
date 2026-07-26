//! Unit tests for `domain/conversation.rs`.

use sage_service::domain::conversation::*;

#[test]
fn test_token_estimation() {
    assert_eq!(ConversationContext::estimate_tokens("hello"), 2);
    assert_eq!(ConversationContext::estimate_tokens(""), 0);
}

#[test]
fn test_context_window_management() {
    let mut ctx = ConversationContext::new(100);

    let msg1 = ConversationMessage {
        id: "1".to_string(),
        role: "user".to_string(),
        content: "Hello, how are you?".to_string(),
        parent_id: None,
        created_at: chrono::Utc::now(),
    };

    let msg2 = ConversationMessage {
        id: "2".to_string(),
        role: "assistant".to_string(),
        content: "I'm doing well, thank you for asking!".to_string(),
        parent_id: Some("1".to_string()),
        created_at: chrono::Utc::now(),
    };

    ctx.add_message(msg1.clone());
    ctx.add_message(msg2.clone());

    let stats = ctx.get_token_stats();
    assert_eq!(stats.total_messages, 2);
    assert!(stats.total_tokens > 0);
}

#[test]
fn test_conversation_branching() {
    let mut ctx = ConversationContext::new(1000);

    let msg1 = ConversationMessage {
        id: "1".to_string(),
        role: "user".to_string(),
        content: "Question".to_string(),
        parent_id: None,
        created_at: chrono::Utc::now(),
    };

    let msg2a = ConversationMessage {
        id: "2a".to_string(),
        role: "assistant".to_string(),
        content: "Answer A".to_string(),
        parent_id: Some("1".to_string()),
        created_at: chrono::Utc::now(),
    };

    let msg2b = ConversationMessage {
        id: "2b".to_string(),
        role: "assistant".to_string(),
        content: "Answer B (different)".to_string(),
        parent_id: Some("1".to_string()),
        created_at: chrono::Utc::now(),
    };

    ctx.add_message(msg1);
    ctx.add_message(msg2a);
    ctx.add_message(msg2b);

    // Both branches should be available
    let branch_a = ctx.get_messages_by_id(Some("2a"));
    let branch_b = ctx.get_messages_by_id(Some("2b"));

    assert_eq!(branch_a.len(), 2); // Root + branch_a
    assert_eq!(branch_b.len(), 2); // Root + branch_b
}
