//! Unit tests for `routers/ui/context_builder.rs`.

use actix_web::web::Data;
use quench_db::InMemoryDb;
use quench_db::prelude::{Crud, Db};
use sage_service::domain::conversation::ConversationContext;
use sage_service::domain::models::Message;
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

#[test]
fn to_json_reports_every_field() {
    let usage = TokenUsageInfo {
        total_messages: 3,
        total_tokens: 400,
        max_tokens: 2048,
        system_tokens: 200,
        near_limit: true,
        at_limit: false,
        utilization_percent: 19.5,
    };

    let json = usage.to_json();
    assert_eq!(json["total_messages"], 3);
    assert_eq!(json["total_tokens"], 400);
    assert_eq!(json["max_tokens"], 2048);
    assert_eq!(json["system_tokens"], 200);
    assert_eq!(json["near_limit"], true);
    assert_eq!(json["at_limit"], false);
}

#[test]
fn get_context_for_llm_reports_the_systems_token_count() {
    let mut ctx = ConversationContext::new(2048);
    ctx.add_message(sage_service::domain::conversation::ConversationMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        content: "hello there".to_string(),
        parent_id: None,
        created_at: chrono::Utc::now(),
    });

    let (messages, usage) = get_context_for_llm(&ctx, "you are a helpful assistant");
    assert!(!messages.is_empty());
    assert!(usage.system_tokens > 0);
    assert_eq!(usage.total_messages, 1);
}

async fn seeded_db(conversation_id: &str) -> Db {
    let db = Db::InMemory(InMemoryDb::new());
    let repo = db.repository::<Message>();
    repo.create(&Message {
        id: "m2".to_string(),
        conversation_id: conversation_id.to_string(),
        parent_id: Some("m1".to_string()),
        role: "assistant".to_string(),
        content: "second".to_string(),
        created_at: "2026-01-01T00:00:01Z".to_string(),
    })
    .await
    .unwrap();
    repo.create(&Message {
        id: "m1".to_string(),
        conversation_id: conversation_id.to_string(),
        parent_id: None,
        role: "user".to_string(),
        content: "first".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .await
    .unwrap();
    // A message from an unrelated conversation must not leak into the built context.
    repo.create(&Message {
        id: "other".to_string(),
        conversation_id: "some-other-conversation".to_string(),
        parent_id: None,
        role: "user".to_string(),
        content: "unrelated".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
    })
    .await
    .unwrap();
    db
}

#[actix_web::test]
async fn build_conversation_context_sorts_and_scopes_to_one_conversation() {
    let db = Data::new(seeded_db("conv-1").await);

    let ctx = build_conversation_context(&db, "conv-1", 2048)
        .await
        .expect("build context");

    let (messages, _) = ctx.get_context_messages("system");
    let contents: Vec<&str> = messages.iter().map(|m| m.content.as_str()).collect();
    assert!(contents.contains(&"first"));
    assert!(contents.contains(&"second"));
    assert!(!contents.contains(&"unrelated"));

    let first_idx = contents.iter().position(|c| *c == "first").unwrap();
    let second_idx = contents.iter().position(|c| *c == "second").unwrap();
    assert!(
        first_idx < second_idx,
        "messages must be sorted by creation time"
    );
}
