use quench_db::prelude::Model;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub messages: String, // JSON serialized Vec<crate::clients::vllm::ChatMessage>
    pub updated_at: String, // ISO 8601 string
}

impl Model for Conversation {
    fn table_name() -> String {
        let schema = envmnt::get_or("DB_SCHEMA", "public");
        format!("{}.conversations", schema)
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "title", "messages", "updated_at"]
    }

    fn primary_key_name() -> String {
        "id".to_string()
    }
}
