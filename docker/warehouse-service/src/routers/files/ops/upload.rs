//! `PUT /api/v1/files/{storage}/file?path=…` - store a file.
//!
//! The body is streamed to disk rather than buffered, so a large upload costs
//! a file descriptor and a 64KB buffer rather than its own size in memory,
//! and the size limit is enforced as the bytes arrive instead of after the
//! last one - true for both storage kinds below, which differ only in what
//! happens once the whole body has landed and its digest is known.

use super::{ResolvedStorage, authorize, dynamic_path, error, forbidden, resolve_storage};
use crate::domain::storage_file;
use crate::routers::files::{FileQuery, dynamic, max_file_bytes};
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, put, web};
use futures_util::StreamExt;
use quench_db::prelude::Db;
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
#[tracing::instrument(skip(body, request))]
pub async fn handle(
    request: HttpRequest,
    db: web::Data<Db>,
    storage: web::Path<String>,
    query: web::Query<FileQuery>,
    mut body: web::Payload,
) -> impl Responder {
    let storage_name = storage.into_inner();
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return *response,
    };

    if !authorize(&request, &resolved, "write") {
        return forbidden("write access to this storage is required");
    }

    match resolved {
        ResolvedStorage::Static(storage) => handle_static(storage, &query.path, &mut body).await,
        ResolvedStorage::Dynamic(storage) => {
            handle_dynamic(&db, &storage, &query.path, &mut body).await
        }
    }
}

async fn handle_static(
    storage: &'static crate::routers::files::Storage,
    path: &str,
    body: &mut web::Payload,
) -> HttpResponse {
    let target = match super::static_target_or_error(storage, path).await {
        Ok(target) => target,
        Err(response) => return *response,
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

    let outcome = stream_to_disk(body, &staging, max_file_bytes()).await;

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
        path: path.to_string(),
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

async fn handle_dynamic(
    db: &Db,
    storage: &crate::domain::storage::DynamicStorage,
    path: &str,
    body: &mut web::Payload,
) -> HttpResponse {
    let path = match dynamic_path(path) {
        Ok(path) => path,
        Err(response) => return *response,
    };

    let Some(root) = dynamic::root() else {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "this deployment has no DYNAMIC_STORAGE_ROOT configured",
        );
    };

    let limit = storage
        .max_file_bytes
        .map(|bytes| bytes as u64)
        .unwrap_or_else(max_file_bytes);
    let staging = dynamic::staging_path(&root);
    if let Some(parent) = staging.parent()
        && tokio::fs::create_dir_all(parent).await.is_err()
    {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not create the staging directory",
        );
    }

    let outcome = stream_to_disk(body, &staging, limit).await;
    let (size, digest) = match outcome {
        Ok(result) => result,
        Err(problem) => {
            let _ = tokio::fs::remove_file(&staging).await;
            return problem.into_response();
        }
    };

    let blob_path = dynamic::blob_path(&root, &digest);

    let put = storage_file::put_file(
        db,
        &storage.name,
        &path,
        &digest,
        size as i64,
        &staging,
        &blob_path,
    )
    .await;

    match put {
        Ok(outcome) => {
            let stored = Stored {
                path,
                size,
                digest: format!("sha256:{digest}"),
            };

            tracing::info!(
                "stored {} bytes at `{}` in dynamic storage `{}`",
                size,
                stored.path,
                storage.name
            );

            if outcome.existed {
                HttpResponse::Ok().json(stored)
            } else {
                HttpResponse::Created().json(stored)
            }
        }
        Err(problem) => {
            let _ = tokio::fs::remove_file(&staging).await;
            match problem {
                crate::domain::db::StorageError::QuotaExceeded => {
                    error(StatusCode::INSUFFICIENT_STORAGE, "storage quota exceeded")
                }
                crate::domain::db::StorageError::NoSuchStorage(_) => {
                    error(StatusCode::NOT_FOUND, "no such storage")
                }
                other => {
                    tracing::error!("dynamic upload failed: {other}");
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "could not store the file",
                    )
                }
            }
        }
    }
}

/// What went wrong while the body was arriving.
enum StreamError {
    TooLarge(u64),
    Read,
    Write,
}

impl StreamError {
    fn into_response(self) -> HttpResponse {
        match self {
            Self::TooLarge(limit) => error(
                StatusCode::PAYLOAD_TOO_LARGE,
                &format!("file exceeds the {limit}-byte limit"),
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
    limit: u64,
) -> Result<(u64, String), StreamError> {
    let mut file = tokio::fs::File::create(staging)
        .await
        .map_err(|_| StreamError::Write)?;

    let mut size: u64 = 0;
    let mut hasher = Sha256::new();

    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| StreamError::Read)?;

        size = size.saturating_add(chunk.len() as u64);
        if size > limit {
            return Err(StreamError::TooLarge(limit));
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
pub fn staging_path(target: &Path) -> PathBuf {
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
