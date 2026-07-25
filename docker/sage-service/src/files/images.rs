use crate::clients::vllm::ChatMessage;
use crate::domain::models::File;
use crate::files::is_image_mime;
use quench_db::prelude::{Crud, Db};
use std::collections::HashMap;

fn db_schema() -> String {
    envmnt::get_or("DB_SCHEMA", "sage")
}

/// Upper bound on images sent to the model in one request, across the current
/// message and history. Must not exceed the vLLM instance's
/// `--limit-mm-per-prompt` image limit or requests with images will fail.
pub fn max_images_per_request() -> usize {
    envmnt::get_u64("SAGE_MAX_IMAGES_PER_REQUEST", 2) as usize
}

/// Rough prompt-budget cost of one image. Actual vision token counts depend
/// on model and resolution; this is deliberately conservative.
pub fn image_token_estimate() -> usize {
    envmnt::get_u64("SAGE_IMAGE_TOKEN_ESTIMATE", 1024) as usize
}

fn to_data_uri(mime: &str, data: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(data)
    )
}

async fn load_blob(db: &Db, file_id: &str) -> Option<Vec<u8>> {
    match db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!("SELECT data FROM {schema}.file_blobs WHERE file_id = $1");
            sqlx::query_as::<_, (Vec<u8>,)>(sqlx::AssertSqlSafe(query.as_str()))
                .bind(file_id)
                .fetch_optional(pg_db.pool())
                .await
                .ok()
                .flatten()
                .map(|(data,)| data)
        }
        Db::InMemory(_) => None,
    }
}

/// Data URIs of the image files among `file_ids` owned by `username`, in the
/// given order. Non-image and foreign files are silently skipped.
pub async fn load_staged_images(db: &Db, file_ids: &[String], username: &str) -> Vec<String> {
    let mut images = Vec::new();
    let repo = db.repository::<File>();
    for id in file_ids {
        if let Ok(Some(f)) = repo.read(id).await
            && f.owner == username
            && is_image_mime(&f.mime_type)
            && let Some(data) = load_blob(db, id).await
        {
            images.push(to_data_uri(&f.mime_type, &data));
        }
    }
    images
}

/// Image data URIs attached to each of the given message ids, in attachment
/// order. Messages without image attachments have no entry.
pub async fn load_images_for_messages(
    db: &Db,
    message_ids: &[String],
) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    if message_ids.is_empty() {
        return map;
    }
    let Db::Postgres(pg_db) = db else {
        return map;
    };
    let schema = db_schema();
    let query = format!(
        "SELECT f.message_id, f.mime_type, b.data \
         FROM {schema}.files f \
         JOIN {schema}.file_blobs b ON b.file_id = f.id \
         WHERE f.message_id = ANY($1) AND f.mime_type LIKE 'image/%' \
         ORDER BY f.created_at"
    );
    match sqlx::query_as::<_, (String, String, Vec<u8>)>(sqlx::AssertSqlSafe(query.as_str()))
        .bind(message_ids)
        .fetch_all(pg_db.pool())
        .await
    {
        Ok(rows) => {
            for (message_id, mime, data) in rows {
                map.entry(message_id)
                    .or_default()
                    .push(to_data_uri(&mime, &data));
            }
        }
        Err(e) => {
            tracing::error!("Failed to load message image attachments: {}", e);
        }
    }
    map
}

/// Enforce the per-request image cap, preferring the newest messages: walk the
/// request back-to-front and drop any images beyond `max`.
pub fn cap_images(messages: &mut [ChatMessage], max: usize) {
    let mut remaining = max;
    for msg in messages.iter_mut().rev() {
        let Some(images) = &mut msg.images else {
            continue;
        };
        if images.len() > remaining {
            images.truncate(remaining);
        }
        remaining -= images.len();
        if images.is_empty() {
            msg.images = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(images: Option<Vec<&str>>) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: "text".to_string(),
            tool_calls: None,
            images: images.map(|v| v.into_iter().map(String::from).collect()),
        }
    }

    #[test]
    fn cap_keeps_newest_images() {
        let mut messages = vec![
            msg(Some(vec!["old-1", "old-2"])),
            msg(None),
            msg(Some(vec!["new-1"])),
        ];
        cap_images(&mut messages, 2);
        assert_eq!(
            messages[0].images.as_deref(),
            Some(&["old-1".to_string()][..])
        );
        assert_eq!(messages[1].images, None);
        assert_eq!(
            messages[2].images.as_deref(),
            Some(&["new-1".to_string()][..])
        );
    }

    #[test]
    fn cap_zero_strips_all_images() {
        let mut messages = vec![msg(Some(vec!["a"])), msg(Some(vec!["b"]))];
        cap_images(&mut messages, 0);
        assert!(messages.iter().all(|m| m.images.is_none()));
    }

    #[test]
    fn cap_under_limit_is_untouched() {
        let mut messages = vec![msg(Some(vec!["a"]))];
        cap_images(&mut messages, 4);
        assert_eq!(messages[0].images.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn data_uri_format() {
        assert_eq!(
            to_data_uri("image/png", b"abc"),
            "data:image/png;base64,YWJj"
        );
    }
}
