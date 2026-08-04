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

/// File statuses: "uploaded" (stored, not yet processed), "processing", "ready" (chunks + embeddings available), "failed".
#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct File {
    pub id: String,
    pub owner: String,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: i64,
    pub conversation_id: Option<String>,
    pub project_id: Option<String>,
    /// The user message this file was attached to; NULL while still staged in the composer.
    pub message_id: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: String, // ISO 8601 string
    pub updated_at: String, // ISO 8601 string
}

impl Model for File {
    fn table_name() -> String {
        let schema = envmnt::get_or("DB_SCHEMA", "sage");
        format!("{}.files", schema)
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "id",
            "owner",
            "file_name",
            "mime_type",
            "file_size",
            "conversation_id",
            "project_id",
            "message_id",
            "status",
            "error_message",
            "created_at",
            "updated_at",
        ]
    }

    fn primary_key_name() -> String {
        "id".to_string()
    }
}

/// The `embedding` vector column is intentionally not part of this model; it's written and queried through raw SQL against pgvector.
#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct FileChunk {
    pub id: String,
    pub file_id: String,
    pub chunk_index: i32,
    pub content: String,
    pub embedding_model: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String, // ISO 8601 string
}

impl Model for FileChunk {
    fn table_name() -> String {
        let schema = envmnt::get_or("DB_SCHEMA", "sage");
        format!("{}.file_chunks", schema)
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "id",
            "file_id",
            "chunk_index",
            "content",
            "embedding_model",
            "metadata",
            "created_at",
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
