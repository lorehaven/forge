use crate::clients::switchboard::{SwitchboardClient, VllmInstance};
use crate::clients::vllm::VllmClient;
use quench_db::prelude::Db;

pub struct EmbedderConfig {
    /// Model name to embed with; empty string disables embedding.
    pub model: String,
    pub batch_size: usize,
    /// Must match the dimension of the file_chunks.embedding column.
    pub dimension: usize,
}

impl EmbedderConfig {
    pub fn from_env() -> Self {
        Self {
            model: envmnt::get_or("SAGE_EMBEDDING_MODEL", "Qwen/Qwen3-Embedding-0.6B"),
            batch_size: envmnt::get_u64("SAGE_EMBEDDING_BATCH_SIZE", 32) as usize,
            dimension: envmnt::get_u64("SAGE_EMBEDDING_DIMENSION", 1024) as usize,
        }
    }
}

pub enum EmbedOutcome {
    /// Embedding is disabled or no embedding model instance is running; chunks stay without vectors until a reprocess fills them in.
    Skipped(String),
    Embedded(usize),
}

fn db_schema() -> String {
    envmnt::get_or("DB_SCHEMA", "sage")
}

async fn find_running_instance(
    switchboard: &SwitchboardClient,
    model: &str,
) -> Result<VllmInstance, String> {
    let instances = switchboard
        .get_vllm_instances()
        .await
        .map_err(|e| format!("switchboard unavailable: {}", e))?;
    instances
        .into_iter()
        .find(|i| i.model == model && i.status == "running")
        .ok_or_else(|| format!("no running instance of embedding model '{}'", model))
}

/// Embed arbitrary texts with the configured embedding model; errors if embedding is disabled or no model instance is running.
pub async fn embed_texts(
    switchboard: &SwitchboardClient,
    vllm: &VllmClient,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>, String> {
    let config = EmbedderConfig::from_env();
    if config.model.is_empty() {
        return Err("embedding disabled (SAGE_EMBEDDING_MODEL is empty)".to_string());
    }
    let instance = find_running_instance(switchboard, &config.model).await?;
    vllm.embeddings(&instance.host, instance.port, &config.model, texts)
        .await
        .map_err(|e| format!("embedding request failed: {}", e))
}

/// Embed all chunks of a file that were just written by the pipeline.
pub async fn embed_file_chunks(
    db: &Db,
    switchboard: &SwitchboardClient,
    vllm: &VllmClient,
    file_id: &str,
) -> Result<EmbedOutcome, String> {
    let config = EmbedderConfig::from_env();
    if config.model.is_empty() {
        return Ok(EmbedOutcome::Skipped(
            "embedding disabled (SAGE_EMBEDDING_MODEL is empty)".to_string(),
        ));
    }

    let Db::Postgres(pg_db) = db else {
        return Ok(EmbedOutcome::Skipped(
            "embedding requires a Postgres database".to_string(),
        ));
    };

    let instance = match find_running_instance(switchboard, &config.model).await {
        Ok(instance) => instance,
        Err(reason) => return Ok(EmbedOutcome::Skipped(reason)),
    };

    let schema = db_schema();
    let select = format!(
        "SELECT id, content FROM {schema}.file_chunks WHERE file_id = $1 ORDER BY chunk_index"
    );
    let chunks: Vec<(String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(select.as_str()))
        .bind(file_id)
        .fetch_all(pg_db.pool())
        .await
        .map_err(|e| e.to_string())?;

    let update = format!(
        "UPDATE {schema}.file_chunks SET embedding = $1::vector, embedding_model = $2 WHERE id = $3"
    );

    let mut embedded = 0;
    for batch in chunks.chunks(config.batch_size.max(1)) {
        let inputs: Vec<String> = batch.iter().map(|(_, content)| content.clone()).collect();
        let vectors = vllm
            .embeddings(&instance.host, instance.port, &config.model, inputs)
            .await
            .map_err(|e| format!("embedding request failed: {}", e))?;

        for ((chunk_id, _), vector) in batch.iter().zip(vectors) {
            if vector.len() != config.dimension {
                return Err(format!(
                    "embedding dimension mismatch: model returned {}, expected {} \
                     (SAGE_EMBEDDING_DIMENSION / file_chunks.embedding column)",
                    vector.len(),
                    config.dimension
                ));
            }
            sqlx::query(sqlx::AssertSqlSafe(update.as_str()))
                .bind(vector_literal(&vector))
                .bind(&config.model)
                .bind(chunk_id)
                .execute(pg_db.pool())
                .await
                .map_err(|e| e.to_string())?;
            embedded += 1;
        }
    }

    Ok(EmbedOutcome::Embedded(embedded))
}

/// pgvector text literal: "[0.1,0.2,...]".
pub fn vector_literal(vector: &[f32]) -> String {
    let mut out = String::with_capacity(vector.len() * 10 + 2);
    out.push('[');
    for (i, v) in vector.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&v.to_string());
    }
    out.push(']');
    out
}
