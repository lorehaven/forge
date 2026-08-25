//! `GET`/`HEAD /api/v1/files/{storage}/file?path=…` - fetch a file back.
//!
//! Streamed off disk in chunks rather than read into a `Vec`, so serving a
//! large file does not cost its size in memory - and several concurrent
//! downloads do not cost their combined size. A static storage streams
//! straight from its own path; a dynamic storage resolves `path` to a blob
//! digest first and streams from the shared blob store instead.

use super::{
    ResolvedStorage, authorize, dynamic_path, error, forbidden, not_found, resolve_storage,
};
use crate::domain::storage_file;
use crate::routers::files::{FileQuery, dynamic};
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, get, head, web};
use bytes::Bytes;
use quench_db::prelude::Db;
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;
use tokio_stream::wrappers::ReceiverStream;

/// How much is read from disk, and handed to the network, at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// How many chunks may sit between the reader and a slow client.
///
/// Small on purpose: the channel is the only thing holding file content in
/// memory, and a client that stops reading should stall the reader rather than
/// let it pull the whole file into a queue.
const CHUNK_BUFFER: usize = 8;

#[get("/{storage}/file")]
#[tracing::instrument(skip(request))]
pub async fn handle(
    request: HttpRequest,
    db: web::Data<Db>,
    storage: web::Path<String>,
    query: web::Query<FileQuery>,
) -> impl Responder {
    let storage_name = storage.into_inner();
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return *response,
    };

    if !authorize(&request, &resolved, "read") {
        return forbidden("read access to this storage is required");
    }

    let target = match resolved {
        ResolvedStorage::Static(storage) => {
            match super::static_target_or_error(storage, &query.path).await {
                Ok(target) => target,
                Err(response) => return *response,
            }
        }
        ResolvedStorage::Dynamic(storage) => {
            let path = match dynamic_path(&query.path) {
                Ok(path) => path,
                Err(response) => return *response,
            };
            let Some(root) = dynamic::root() else {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "this deployment has no DYNAMIC_STORAGE_ROOT configured",
                );
            };
            match storage_file::read_file(&db, &storage.name, &path).await {
                Ok(Some((sha256, _size))) => dynamic::blob_path(&root, &sha256),
                Ok(None) => return not_found("no such file"),
                Err(problem) => {
                    tracing::error!("dynamic download lookup failed: {problem}");
                    return error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed");
                }
            }
        }
    };

    stream_file(&target).await
}

#[head("/{storage}/file")]
#[tracing::instrument(skip(request))]
pub async fn head(
    request: HttpRequest,
    db: web::Data<Db>,
    storage: web::Path<String>,
    query: web::Query<FileQuery>,
) -> impl Responder {
    let storage_name = storage.into_inner();
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return *response,
    };

    if !authorize(&request, &resolved, "read") {
        return forbidden("read access to this storage is required");
    }

    let target = match resolved {
        ResolvedStorage::Static(storage) => {
            match super::static_target_or_error(storage, &query.path).await {
                Ok(target) => target,
                Err(response) => return *response,
            }
        }
        ResolvedStorage::Dynamic(storage) => {
            let path = match dynamic_path(&query.path) {
                Ok(path) => path,
                Err(response) => return *response,
            };
            let Some(root) = dynamic::root() else {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "this deployment has no DYNAMIC_STORAGE_ROOT configured",
                );
            };
            match storage_file::read_file(&db, &storage.name, &path).await {
                Ok(Some((sha256, _size))) => dynamic::blob_path(&root, &sha256),
                Ok(None) => return not_found("no such file"),
                Err(problem) => {
                    tracing::error!("dynamic download lookup failed: {problem}");
                    return error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed");
                }
            }
        }
    };

    match tokio::fs::metadata(&target).await {
        Ok(metadata) if metadata.is_file() => HttpResponse::Ok()
            .content_type("application/octet-stream")
            .append_header(("Content-Length", metadata.len()))
            .finish(),
        _ => error(StatusCode::NOT_FOUND, "no such file"),
    }
}

async fn stream_file(target: &Path) -> HttpResponse {
    let metadata = match tokio::fs::metadata(target).await {
        Ok(metadata) => metadata,
        Err(_) => return not_found("no such file"),
    };

    // A directory is not a file, and answering with one would mean deciding
    // what "the content of a directory" is. `GET /{storage}` lists instead.
    if !metadata.is_file() {
        return not_found("no such file");
    }

    let file = match tokio::fs::File::open(target).await {
        Ok(file) => file,
        Err(_) => return not_found("no such file"),
    };

    HttpResponse::Ok()
        .content_type("application/octet-stream")
        .append_header(("Content-Length", metadata.len()))
        .append_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", download_name(target)),
        ))
        .streaming(read_stream(file))
}

/// The file's own name, for a browser saving it.
///
/// Quotes and control bytes are dropped rather than escaped: the name only has
/// to be a usable suggestion, and a header that cannot be parsed is worse than
/// one that suggests something slightly different.
pub fn download_name(target: &Path) -> String {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());

    let cleaned: String = name
        .chars()
        .filter(|c| *c != '"' && *c != '\\' && !c.is_control())
        .collect();

    if cleaned.is_empty() {
        "download".to_string()
    } else {
        cleaned
    }
}

/// Reads the file on a task of its own, handing chunks over a bounded channel.
///
/// The bound is what applies backpressure: once `CHUNK_BUFFER` chunks are
/// waiting, the reader blocks until the client has taken one.
fn read_stream(mut file: tokio::fs::File) -> ReceiverStream<Result<Bytes, std::io::Error>> {
    let (sender, receiver) = tokio::sync::mpsc::channel(CHUNK_BUFFER);

    tokio::spawn(async move {
        let mut buffer = vec![0u8; CHUNK_BYTES];
        loop {
            match file.read(&mut buffer).await {
                Ok(0) => break,
                Ok(read) => {
                    if sender
                        .send(Ok(Bytes::copy_from_slice(&buffer[..read])))
                        .await
                        .is_err()
                    {
                        // The client hung up. Nothing to report to it, and the
                        // rest of the file is no longer wanted.
                        break;
                    }
                }
                Err(problem) => {
                    let _ = sender.send(Err(problem)).await;
                    break;
                }
            }
        }
    });

    ReceiverStream::new(receiver)
}

/// Kept for the delete handler, which wants the same "is this a file" answer.
pub async fn is_file(target: &PathBuf) -> bool {
    tokio::fs::metadata(target)
        .await
        .is_ok_and(|metadata| metadata.is_file())
}
