//! `domain/models.rs`'s `Model` impls - `table_name`/`columns`/`primary_key_name`.

use crate::env_support::env_lock;
use quench_db::prelude::Model;
use sage_service::domain::models::{Conversation, File, FileChunk, Message, Project};

#[test]
fn table_names_default_to_the_sage_schema() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::remove_var("DB_SCHEMA") };

    assert_eq!(Project::table_name(), "sage.projects");
    assert_eq!(Conversation::table_name(), "sage.conversations");
    assert_eq!(File::table_name(), "sage.files");
    assert_eq!(FileChunk::table_name(), "sage.file_chunks");
    assert_eq!(Message::table_name(), "sage.messages");
}

#[test]
fn table_names_honor_a_db_schema_override() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe { std::env::set_var("DB_SCHEMA", "custom_schema") };

    assert_eq!(Project::table_name(), "custom_schema.projects");
    assert_eq!(Conversation::table_name(), "custom_schema.conversations");

    unsafe { std::env::remove_var("DB_SCHEMA") };
}

#[test]
fn primary_key_is_id_for_every_model() {
    assert_eq!(Project::primary_key_name(), "id");
    assert_eq!(Conversation::primary_key_name(), "id");
    assert_eq!(File::primary_key_name(), "id");
    assert_eq!(FileChunk::primary_key_name(), "id");
    assert_eq!(Message::primary_key_name(), "id");
}

#[test]
fn columns_list_every_field_in_declaration_order() {
    assert_eq!(
        Project::columns(),
        vec!["id", "name", "owner", "created_at", "updated_at"]
    );
    assert_eq!(
        Conversation::columns(),
        vec![
            "id",
            "title",
            "active_message_id",
            "owner",
            "project_id",
            "updated_at"
        ]
    );
    assert_eq!(
        File::columns(),
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
            "updated_at"
        ]
    );
    assert_eq!(
        FileChunk::columns(),
        vec![
            "id",
            "file_id",
            "chunk_index",
            "content",
            "embedding_model",
            "metadata",
            "created_at"
        ]
    );
    assert_eq!(
        Message::columns(),
        vec![
            "id",
            "conversation_id",
            "parent_id",
            "role",
            "content",
            "created_at"
        ]
    );
}
