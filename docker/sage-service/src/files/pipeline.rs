use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::VllmClient;
use crate::domain::models::{File, FileChunk};
use crate::files::embedder::{self, EmbedOutcome};
use crate::files::{STATUS_FAILED, STATUS_PROCESSING, STATUS_READY, chunker, extractor};
use chrono::Utc;
use quench_db::prelude::{Crud, Db};
use uuid::Uuid;

fn db_schema() -> String {
    envmnt::get_or("DB_SCHEMA", "sage")
}

/// Run extraction, chunking, and embedding for an uploaded file in the background.
pub fn spawn_processing(db: Db, switchboard: SwitchboardClient, vllm: VllmClient, file_id: String) {
    tokio::spawn(async move {
        match process_file(&db, &switchboard, &vllm, &file_id).await {
            Ok(chunk_count) => {
                tracing::info!("File {} processed into {} chunks", file_id, chunk_count);
            }
            Err(e) => {
                tracing::error!("Processing of file {} failed: {}", file_id, e);
                if let Err(update_err) = set_status(&db, &file_id, STATUS_FAILED, Some(&e)).await {
                    tracing::error!("Failed to mark file {} as failed: {}", file_id, update_err);
                }
            }
        }
    });
}

pub async fn process_file(
    db: &Db,
    switchboard: &SwitchboardClient,
    vllm: &VllmClient,
    file_id: &str,
) -> Result<usize, String> {
    let file = set_status(db, file_id, STATUS_PROCESSING, None).await?;

    let blob = load_blob(db, file_id).await?;
    let segments = extractor::extract_segments(&file.mime_type, &blob)?;

    let config = chunker::ChunkerConfig::from_env();
    let chunks = chunker::chunk_segments(&segments, &config);
    if chunks.is_empty() {
        return Err("No text content could be extracted from the file".to_string());
    }

    delete_chunks(db, file_id).await?;

    let repo = db.repository::<FileChunk>();
    let now = Utc::now().to_rfc3339();
    for (index, chunk) in chunks.iter().enumerate() {
        let metadata =
            (!chunk.metadata.is_empty()).then(|| serde_json::Value::Object(chunk.metadata.clone()));
        let record = FileChunk {
            id: Uuid::new_v4().to_string(),
            file_id: file_id.to_string(),
            chunk_index: index as i32,
            content: chunk.content.clone(),
            embedding_model: None,
            metadata,
            created_at: now.clone(),
        };
        repo.create(&record).await.map_err(|e| e.to_string())?;
    }

    match embedder::embed_file_chunks(db, switchboard, vllm, file_id).await? {
        EmbedOutcome::Embedded(count) => {
            tracing::info!("Embedded {} chunks for file {}", count, file_id);
        }
        EmbedOutcome::Skipped(reason) => {
            // The file is still usable without vectors; a later reprocess fills them in.
            tracing::warn!(
                "Chunks of file {} stored without embeddings: {}",
                file_id,
                reason
            );
        }
    }

    set_status(db, file_id, STATUS_READY, None).await?;
    Ok(chunks.len())
}

async fn set_status(
    db: &Db,
    file_id: &str,
    status: &str,
    error_message: Option<&str>,
) -> Result<File, String> {
    let repo = db.repository::<File>();
    let mut file = repo
        .read(file_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("File {} not found", file_id))?;
    file.status = status.to_string();
    file.error_message = error_message.map(|s| s.to_string());
    file.updated_at = Utc::now().to_rfc3339();
    repo.update(&file).await.map_err(|e| e.to_string())
}

async fn load_blob(db: &Db, file_id: &str) -> Result<Vec<u8>, String> {
    match db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!("SELECT data FROM {schema}.file_blobs WHERE file_id = $1");
            let row: Option<(Vec<u8>,)> = sqlx::query_as(sqlx::AssertSqlSafe(query.as_str()))
                .bind(file_id)
                .fetch_optional(pg_db.pool())
                .await
                .map_err(|e| e.to_string())?;
            row.map(|(data,)| data)
                .ok_or_else(|| format!("Blob for file {} not found", file_id))
        }
        Db::InMemory(_) => Err("File processing requires a Postgres database".to_string()),
    }
}

async fn delete_chunks(db: &Db, file_id: &str) -> Result<(), String> {
    match db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!("DELETE FROM {schema}.file_chunks WHERE file_id = $1");
            sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
                .bind(file_id)
                .execute(pg_db.pool())
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        Db::InMemory(_) => {
            let repo = db.repository::<FileChunk>();
            let chunks = repo.list().await.map_err(|e| e.to_string())?;
            for chunk in chunks.into_iter().filter(|c| c.file_id == file_id) {
                repo.delete(&chunk.id).await.map_err(|e| e.to_string())?;
            }
            Ok(())
        }
    }
}
