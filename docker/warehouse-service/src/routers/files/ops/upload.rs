//! `PUT /api/v1/files/{storage}/file?path=…` - store a file.
//!
//! The body is the file. It is streamed to disk rather than buffered, so a
//! large artifact costs a file descriptor and a 64KB buffer rather than its own
//! size in memory, and the size limit is enforced as the bytes arrive instead
//! of after the last one.

use super::{error, target_or_error};
use crate::routers::files::{FileQuery, max_file_bytes};
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder, put, web};
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

#[derive(Serialize)]
pub struct Stored {
    pub path: String,
    pub size: u64,
    pub digest: String,
}

#[put("/{storage}/file")]
#[tracing::instrument(skip(body))]
pub async fn handle(
    storage: web::Path<String>,
    query: web::Query<FileQuery>,
    mut body: web::Payload,
) -> impl Responder {
    let storage_name = storage.into_inner();
    let (storage, target) = match target_or_error(&storage_name, &query.path).await {
        Ok(resolved) => resolved,
        // The body is deliberately not drained. A rejected upload should stop
        // costing bandwidth at the moment it is rejected, not after the client
        // has finished sending a file that is going nowhere.
        Err(response) => return response,
    };

    if let Some(parent) = target.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not create the containing directory",
        );
    }

    // A directory already at the target would make the rename below fail with
    // something unhelpful; say what is actually wrong.
    if tokio::fs::metadata(&target).await.is_ok_and(|m| m.is_dir()) {
        return error(
            StatusCode::CONFLICT,
            "a directory already exists at that path",
        );
    }

    let existed = tokio::fs::try_exists(&target).await.unwrap_or(false);

    // Written beside the target and renamed into place at the end. A dropped
    // connection halfway through therefore leaves the previous version intact
    // rather than a truncated file that looks complete.
    let staging = staging_path(&target);

    let outcome = stream_to_disk(&mut body, &staging).await;

    let (size, digest) = match outcome {
        Ok(result) => result,
        Err(problem) => {
            let _ = tokio::fs::remove_file(&staging).await;
            return problem.into_response();
        }
    };

    if tokio::fs::rename(&staging, &target).await.is_err() {
        let _ = tokio::fs::remove_file(&staging).await;
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not store the file",
        );
    }

    let stored = Stored {
        path: query.path.clone(),
        size,
        digest: format!("sha256:{digest}"),
    };

    tracing::info!(
        "stored {} bytes at `{}` in storage `{}`",
        size,
        stored.path,
        storage.name
    );

    // 201 for a new file, 200 for a replacement - so a caller that cares can
    // tell whether it overwrote something without asking first.
    if existed {
        HttpResponse::Ok().json(stored)
    } else {
        HttpResponse::Created().json(stored)
    }
}

/// What went wrong while the body was arriving.
enum StreamError {
    TooLarge,
    Read,
    Write,
}

impl StreamError {
    fn into_response(self) -> HttpResponse {
        match self {
            Self::TooLarge => error(
                StatusCode::PAYLOAD_TOO_LARGE,
                &format!("file exceeds the {}-byte limit", max_file_bytes()),
            ),
            Self::Read => error(StatusCode::BAD_REQUEST, "the upload was interrupted"),
            Self::Write => error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not write the file",
            ),
        }
    }
}

/// Streams the body into `staging`, returning its size and hex SHA-256.
async fn stream_to_disk(
    body: &mut web::Payload,
    staging: &Path,
) -> Result<(u64, String), StreamError> {
    let mut file = tokio::fs::File::create(staging)
        .await
        .map_err(|_| StreamError::Write)?;

    let limit = max_file_bytes();
    let mut size: u64 = 0;
    let mut hasher = Sha256::new();

    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| StreamError::Read)?;

        size = size.saturating_add(chunk.len() as u64);
        if size > limit {
            return Err(StreamError::TooLarge);
        }

        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|_| StreamError::Write)?;
    }

    // Flushed explicitly: the rename that follows would otherwise be ordered
    // against a file whose last bytes are still in the buffer.
    file.flush().await.map_err(|_| StreamError::Write)?;

    Ok((size, hex::encode(hasher.finalize())))
}

/// A sibling of the target, so the rename that follows stays on one filesystem.
///
/// The process id and a counter keep two uploads of the same path from sharing
/// a staging file, which would otherwise interleave their bytes and leave the
/// loser's digest describing the winner's content.
fn staging_path(target: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "upload".to_string());

    let unique = format!(
        ".{name}.{}.{}.part",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );

    match target.parent() {
        Some(parent) => parent.join(unique),
        None => PathBuf::from(unique),
    }
}
