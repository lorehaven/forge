use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::VllmClient;
use crate::files::{STATUS_READY, embedder};
use crate::models::Conversation;
use quench_db::prelude::{Crud, Db};
use uuid::Uuid;

pub struct RagConfig {
    pub auto_inject: bool,
    pub top_k: i64,
    pub similarity_threshold: f64,
    pub max_context_chars: usize,
}

impl RagConfig {
    pub fn from_env() -> Self {
        Self {
            auto_inject: envmnt::is_or("SAGE_RAG_AUTO_INJECT", true),
            top_k: envmnt::get_u64("SAGE_RAG_TOP_K", 4) as i64,
            similarity_threshold: envmnt::get_or("SAGE_RAG_SIMILARITY_THRESHOLD", "0.35")
                .parse()
                .unwrap_or(0.35),
            max_context_chars: envmnt::get_u64("SAGE_RAG_MAX_CONTEXT_CHARS", 2000) as usize,
        }
    }
}

/// Default instruction wrapped around a search query for instruction-tuned
/// embedding models (Qwen3-Embedding et al.). Set the env var to an empty
/// string to embed queries verbatim for plain embedding models.
const DEFAULT_QUERY_INSTRUCTION: &str =
    "Given a search query, retrieve relevant passages from the uploaded documents";

fn query_input(query: &str) -> String {
    let instruction = envmnt::get_or(
        "SAGE_EMBEDDING_QUERY_INSTRUCTION",
        DEFAULT_QUERY_INSTRUCTION,
    );
    if instruction.trim().is_empty() {
        query.to_string()
    } else {
        format!("Instruct: {}\nQuery: {}", instruction, query)
    }
}

#[derive(Debug, Clone)]
pub struct ChunkHit {
    pub file_id: String,
    pub chunk_id: String,
    pub file_name: String,
    pub chunk_index: i32,
    pub content: String,
    pub similarity: f64,
    /// Human-readable location within the file (heading or "p.N"), if known.
    pub detail: Option<String>,
}

type SearchChunkRow = (
    String,
    String,
    String,
    i32,
    String,
    Option<serde_json::Value>,
    f64,
);

type RagContextRow = (String, String, Option<i32>, Option<String>, Option<f64>);

fn db_schema() -> String {
    envmnt::get_or("DB_SCHEMA", "sage")
}

/// Derive a short location label from a chunk's metadata JSON.
fn detail_from_metadata(metadata: &Option<serde_json::Value>) -> Option<String> {
    let obj = metadata.as_ref()?.as_object()?;
    if let Some(heading) = obj.get("heading").and_then(|v| v.as_str()) {
        return Some(heading.to_string());
    }
    if let Some(page) = obj.get("page").and_then(|v| v.as_i64()) {
        return Some(format!("p.{}", page));
    }
    None
}

/// Cosine-search the embedded chunks visible to a conversation (its own
/// files, its project's files, and files of sibling conversations in the
/// same project).
///
/// `project_id_hint` scopes the search when the conversation row does not exist
/// yet (e.g. the first message of a project chat, persisted only after tools
/// run). When the conversation exists its own project link takes precedence.
pub async fn search_chunks(
    db: &Db,
    switchboard: &SwitchboardClient,
    vllm: &VllmClient,
    conversation_id: &str,
    project_id_hint: Option<&str>,
    query: &str,
    top_k: i64,
) -> Result<Vec<ChunkHit>, String> {
    let Db::Postgres(pg_db) = db else {
        return Err("file search requires a Postgres database".to_string());
    };

    // A missing conversation is not fatal: the query then relies on the project
    // scope (see the WHERE clause below), which the hint supplies.
    let conversation = db
        .repository::<Conversation>()
        .read(conversation_id)
        .await
        .map_err(|e| e.to_string())?;
    let project_id = conversation
        .as_ref()
        .and_then(|c| c.project_id.clone())
        .or_else(|| project_id_hint.map(str::to_string));

    let vectors = embedder::embed_texts(switchboard, vllm, vec![query_input(query)]).await?;
    let query_vector = vectors
        .into_iter()
        .next()
        .ok_or_else(|| "embedding model returned no vector for the query".to_string())?;

    let schema = db_schema();
    let sql = format!(
        "SELECT f.id, fc.id, f.file_name, fc.chunk_index, fc.content, fc.metadata, \
                1 - (fc.embedding <=> $1::vector) AS similarity \
         FROM {schema}.file_chunks fc \
         JOIN {schema}.files f ON fc.file_id = f.id \
         LEFT JOIN {schema}.conversations c ON f.conversation_id = c.id \
         WHERE fc.embedding IS NOT NULL \
           AND f.status = '{STATUS_READY}' \
           AND (f.conversation_id = $2 \
                OR ($3::text IS NOT NULL AND (f.project_id = $3 OR c.project_id = $3))) \
         ORDER BY fc.embedding <=> $1::vector \
         LIMIT $4"
    );

    let rows: Vec<SearchChunkRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(embedder::vector_literal(&query_vector))
        .bind(conversation_id)
        .bind(&project_id)
        .bind(top_k.max(1))
        .fetch_all(pg_db.pool())
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(
            |(file_id, chunk_id, file_name, chunk_index, content, metadata, similarity)| {
                let detail = detail_from_metadata(&metadata);
                ChunkHit {
                    file_id,
                    chunk_id,
                    file_name,
                    chunk_index,
                    content,
                    similarity,
                    detail,
                }
            },
        )
        .collect())
}

/// A "source" reference stored against an assistant message for attribution.
#[derive(Debug, Clone)]
pub struct RagSource {
    pub file_name: String,
    pub chunk_index: Option<i32>,
    pub detail: Option<String>,
    pub similarity: Option<f64>,
}

/// Persist which chunks fed a message's answer, for UI source attribution.
/// `source` is "auto" (injected into the prompt) or "tool" (file_search).
pub async fn record_rag_contexts(
    db: &Db,
    message_id: &str,
    hits: &[ChunkHit],
    source: &str,
) -> Result<(), String> {
    if hits.is_empty() {
        return Ok(());
    }
    let Db::Postgres(pg_db) = db else {
        return Ok(());
    };
    let schema = db_schema();
    let sql = format!(
        "INSERT INTO {schema}.rag_contexts \
         (id, message_id, file_id, file_name, chunk_index, detail, similarity, source, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    );
    let now = chrono::Utc::now().to_rfc3339();
    for hit in hits {
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(Uuid::new_v4().to_string())
            .bind(message_id)
            .bind(&hit.file_id)
            .bind(&hit.file_name)
            .bind(hit.chunk_index)
            .bind(&hit.detail)
            .bind(hit.similarity)
            .bind(source)
            .bind(&now)
            .execute(pg_db.pool())
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Load the source references for a set of messages, keyed by message id.
pub async fn load_sources_for_messages(
    db: &Db,
    message_ids: &[String],
) -> std::collections::HashMap<String, Vec<RagSource>> {
    let mut map: std::collections::HashMap<String, Vec<RagSource>> =
        std::collections::HashMap::new();
    if message_ids.is_empty() {
        return map;
    }
    let Db::Postgres(pg_db) = db else {
        return map;
    };
    let schema = db_schema();
    let sql = format!(
        "SELECT message_id, file_name, chunk_index, detail, similarity \
         FROM {schema}.rag_contexts \
         WHERE message_id = ANY($1) \
         ORDER BY similarity DESC NULLS LAST"
    );
    let rows: Vec<RagContextRow> = match sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(message_ids)
        .fetch_all(pg_db.pool())
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Failed to load rag_contexts: {}", e);
            return map;
        }
    };
    for (message_id, file_name, chunk_index, detail, similarity) in rows {
        map.entry(message_id).or_default().push(RagSource {
            file_name,
            chunk_index,
            detail,
            similarity,
        });
    }
    map
}

/// Build the system-prompt addition for a conversation with uploaded files:
/// a list of available files plus, when enabled, excerpts relevant to the
/// current user message. Returns the prompt text and the hits that were
/// injected (for source attribution). Returns None when the conversation has
/// no ready files.
pub async fn augment_system_prompt(
    db: &Db,
    switchboard: &SwitchboardClient,
    vllm: &VllmClient,
    conversation_id: &str,
    user_message: &str,
) -> Option<(String, Vec<ChunkHit>)> {
    let conversation = db
        .repository::<Conversation>()
        .read(conversation_id)
        .await
        .ok()??;

    let files = crate::routers::files::visible_files_for_conversation(db, &conversation)
        .await
        .ok()?;
    let ready_files: Vec<_> = files.iter().filter(|f| f.status == STATUS_READY).collect();
    if ready_files.is_empty() {
        return None;
    }

    let mut out = String::from(
        "\n\nUPLOADED FILES\n\
         The user has uploaded the following files to this conversation or its project. \
         Use the file_search tool to look up their content when the question may relate to them:\n",
    );
    for file in &ready_files {
        out.push_str(&format!("- {}\n", file.file_name));
    }

    let config = RagConfig::from_env();
    if !config.auto_inject || user_message.trim().is_empty() {
        return Some((out, Vec::new()));
    }

    let mut injected = Vec::new();
    match search_chunks(
        db,
        switchboard,
        vllm,
        conversation_id,
        conversation.project_id.as_deref(),
        user_message,
        config.top_k,
    )
    .await
    {
        Ok(hits) => {
            let relevant: Vec<_> = hits
                .into_iter()
                .filter(|h| h.similarity >= config.similarity_threshold)
                .collect();
            if !relevant.is_empty() {
                out.push_str(
                    "\nRELEVANT FILE EXCERPTS\n\
                     Excerpts from the uploaded files that may relate to the current message:\n",
                );
                let mut used_chars = 0;
                for hit in relevant {
                    let remaining = config.max_context_chars.saturating_sub(used_chars);
                    if remaining == 0 {
                        break;
                    }
                    let mut content = hit.content.clone();
                    if content.len() > remaining {
                        let mut cut = remaining;
                        while !content.is_char_boundary(cut) {
                            cut -= 1;
                        }
                        content.truncate(cut);
                    }
                    used_chars += content.len();
                    let location = hit
                        .detail
                        .as_ref()
                        .map(|d| format!(" · {}", d))
                        .unwrap_or_default();
                    out.push_str(&format!(
                        "[{}{} · similarity {:.2}]\n{}\n\n",
                        hit.file_name, location, hit.similarity, content
                    ));
                    injected.push(hit);
                }
            }
        }
        Err(reason) => {
            // Search is best-effort here; the file list alone is still useful.
            tracing::debug!(
                "RAG auto-inject skipped for conversation {}: {}",
                conversation_id,
                reason
            );
        }
    }

    Some((out, injected))
}
