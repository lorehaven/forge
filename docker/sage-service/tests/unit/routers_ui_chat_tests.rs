//! Unit tests for `routers/ui/chat.rs`.

use quench_db::prelude::{Crud, Db};
use sage_service::routers::ui::chat::*;

#[actix_web::test]
async fn retrieves_only_the_selected_conversation_branch() {
    let db = Db::InMemory(quench_db::InMemoryDb::new());
    let repo = db.repository::<sage_service::domain::models::Message>();

    for message in [
        sage_service::domain::models::Message {
            id: "root".to_string(),
            conversation_id: "conversation".to_string(),
            parent_id: None,
            role: "user".to_string(),
            content: "question".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        },
        sage_service::domain::models::Message {
            id: "answer-a".to_string(),
            conversation_id: "conversation".to_string(),
            parent_id: Some("root".to_string()),
            role: "assistant".to_string(),
            content: "answer a".to_string(),
            created_at: "2026-01-01T00:00:01Z".to_string(),
        },
        sage_service::domain::models::Message {
            id: "answer-b".to_string(),
            conversation_id: "conversation".to_string(),
            parent_id: Some("root".to_string()),
            role: "assistant".to_string(),
            content: "answer b".to_string(),
            created_at: "2026-01-01T00:00:02Z".to_string(),
        },
    ] {
        repo.create(&message).await.unwrap();
    }

    let branch = get_conversation_messages(&db, Some("answer-b"))
        .await
        .unwrap();
    let siblings = get_siblings(&db, "conversation", Some("root"))
        .await
        .unwrap();

    assert_eq!(
        branch
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["question", "answer b"]
    );
    assert_eq!(
        siblings
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["answer-a", "answer-b"]
    );
}
