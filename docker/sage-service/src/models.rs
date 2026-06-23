use quench_db::prelude::Model;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub created_at: String, // ISO 8601 string
    pub updated_at: String, // ISO 8601 string
}

impl Model for Project {
    fn table_name() -> String {
        let schema = envmnt::get_or("DB_SCHEMA", "sage");
        format!("{}.projects", schema)
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "owner", "created_at", "updated_at"]
    }

    fn primary_key_name() -> String {
        "id".to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub active_message_id: Option<String>,
    pub owner: String,
    pub project_id: Option<String>,
    pub updated_at: String, // ISO 8601 string
}

impl Model for Conversation {
    fn table_name() -> String {
        let schema = envmnt::get_or("DB_SCHEMA", "sage");
        format!("{}.conversations", schema)
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "id",
            "title",
            "active_message_id",
            "owner",
            "project_id",
            "updated_at",
        ]
    }

    fn primary_key_name() -> String {
        "id".to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub content: String,
    pub created_at: String, // ISO 8601 string
}

impl Model for Message {
    fn table_name() -> String {
        let schema = envmnt::get_or("DB_SCHEMA", "sage");
        format!("{}.messages", schema)
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "id",
            "conversation_id",
            "parent_id",
            "role",
            "content",
            "created_at",
        ]
    }

    fn primary_key_name() -> String {
        "id".to_string()
    }
}
