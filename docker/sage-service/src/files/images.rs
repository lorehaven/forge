use crate::clients::vllm::ChatMessage;
use crate::domain::models::File;
use crate::files::is_image_mime;
use quench_db::prelude::{Crud, Db};
use std::collections::HashMap;

fn db_schema() -> String {
    quench_core::env::get_or("DB_SCHEMA", "sage")
}

/// Master switch for image handling (`SAGE_IMAGES_ENABLED`, default true). When false, no
/// image uploads are accepted and none are ever sent to a model, regardless of history.
#[must_use]
pub fn images_enabled() -> bool {
    quench_core::env::get_or("SAGE_IMAGES_ENABLED", "true")
        .parse::<bool>()
        .unwrap_or(true)
}

/// Upper bound on images sent per request, across the current message and history.
///
/// Must not
/// exceed the vLLM instance's `--limit-mm-per-prompt`, or requests with images will fail.
/// Zero when [`images_enabled`] is false, which makes `cap_images` strip every image.
#[must_use]
pub fn max_images_per_request() -> usize {
    if !images_enabled() {
        return 0;
    }
    usize::try_from(quench_core::env::get_parsed::<u64>(
        "SAGE_MAX_IMAGES_PER_REQUEST",
        2,
    ))
    .unwrap_or(2)
}

/// Rough prompt-budget cost of one image; deliberately conservative since actual vision token counts depend on model and resolution.
#[must_use]
pub fn image_token_estimate() -> usize {
    usize::try_from(quench_core::env::get_parsed::<u64>(
        "SAGE_IMAGE_TOKEN_ESTIMATE",
        1024,
    ))
    .unwrap_or(1024)
}

#[must_use]
pub fn to_data_uri(mime: &str, data: &[u8]) -> String {
    format!(
        "data:{};base64,{}",
        mime,
        quench_core::base64::STANDARD.encode(data)
    )
}

async fn load_blob(db: &Db, file_id: &str) -> Option<Vec<u8>> {
    match db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!("SELECT data FROM {schema}.file_blobs WHERE file_id = $1");
            crate::domain::rows::fetch_optional_tuple::<(Vec<u8>,)>(
                pg_db.pool(),
                &query,
                &[&file_id],
            )
            .await
            .ok()
            .flatten()
            .map(|(data,)| data)
        }
        Db::InMemory(_) => None,
    }
}

/// Data URIs of the image files among `file_ids` owned by `username`, in order; non-image and foreign files are silently skipped.
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

/// Image data URIs attached to each message id, in attachment order; messages with no images have no entry.
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
    match crate::domain::rows::fetch_all_tuples::<(String, String, Vec<u8>)>(
        pg_db.pool(),
        &query,
        &[&quench_pg::PgArray(message_ids.to_vec()) as &dyn quench_pg::ToSql],
    )
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
            quench_log::error!("Failed to load message image attachments: {}", e);
        }
    }
    map
}

/// Enforce the per-request image cap, preferring the newest messages: walk back-to-front, dropping any images beyond `max`.
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
