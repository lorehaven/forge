//! `GET`/`HEAD /api/v1/files/{storage}/file?path=…` - streamed off disk in
//! chunks, so serving (and concurrently downloading) a large file stays cheap in memory.

use super::{
    ResolvedStorage, authorize, dynamic_path, error, forbidden, not_found, resolve_storage,
};
use crate::domain::storage_file;
use crate::routers::files::{FileQuery, OptionalClaims, dynamic};
use async_trait::async_trait;
use bytes::Bytes;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Endpoint, FromRequest, Inject, Path, Query, Request, Response, get,
    http::{Method, StatusCode},
};
use std::path::{Path as FsPath, PathBuf};
use tokio::io::AsyncReadExt;
use tokio_stream::wrappers::ReceiverStream;

/// How much is read from disk, and handed to the network, at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// How many chunks may sit between the reader and a slow client - small so a
/// stalled client stalls the reader instead of buffering the whole file.
const CHUNK_BUFFER: usize = 8;

#[get("/api/v1/files/{storage}/file")]
#[tracing::instrument(skip(claims, config, db))]
pub async fn handle(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Path(storage_name): Path<String>,
    Query(query): Query<FileQuery>,
) -> Response {
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };

    if !authorize(claims.as_ref(), &config, &resolved, "read") {
        return forbidden("read access to this storage is required");
    }

    let target = match resolved {
        ResolvedStorage::Static(storage) => {
            match super::static_target_or_error(storage, &query.path).await {
                Ok(target) => target,
                Err(response) => return response,
            }
        }
        ResolvedStorage::Dynamic(storage) => {
            let path = match dynamic_path(&query.path) {
                Ok(path) => path,
                Err(response) => return response,
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

    let inline = query.disposition.as_deref() == Some("inline");
    stream_file(&target, &query.path, inline).await
}

/// No `#[head]` macro exists - hand-expanded from `route_impl` in quench-http-macros.
pub async fn head(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Path(storage_name): Path<String>,
    Query(query): Query<FileQuery>,
) -> Response {
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };

    if !authorize(claims.as_ref(), &config, &resolved, "read") {
        return forbidden("read access to this storage is required");
    }

    let target = match resolved {
        ResolvedStorage::Static(storage) => {
            match super::static_target_or_error(storage, &query.path).await {
                Ok(target) => target,
                Err(response) => return response,
            }
        }
        ResolvedStorage::Dynamic(storage) => {
            let path = match dynamic_path(&query.path) {
                Ok(path) => path,
                Err(response) => return response,
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
        Ok(metadata) if metadata.is_file() => Response::new(StatusCode::OK)
            .header("content-type", "application/octet-stream")
            .header("content-length", metadata.len().to_string()),
        _ => error(StatusCode::NOT_FOUND, "no such file"),
    }
}

#[allow(non_camel_case_types)]
struct __quench_route_head;

#[async_trait]
impl Endpoint for __quench_route_head {
    async fn call(&self, mut req: Request) -> Response {
        let arg0 = match <OptionalClaims as FromRequest>::from_request(&mut req).await {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        let arg1 = match <Inject<JwtConfig> as FromRequest>::from_request(&mut req).await {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        let arg2 = match <Inject<Db> as FromRequest>::from_request(&mut req).await {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        let arg3 = match <Path<String> as FromRequest>::from_request(&mut req).await {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        let arg4 = match <Query<FileQuery> as FromRequest>::from_request(&mut req).await {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        head(arg0, arg1, arg2, arg3, arg4).await
    }
}

quench_http::inventory::submit! {
    quench_http::prelude::RouteRegistration {
        method: Method::HEAD,
        pattern: "/api/v1/files/{storage}/file",
        endpoint: || std::sync::Arc::new(__quench_route_head) as std::sync::Arc<dyn Endpoint>,
    }
}

/// Streams `target` back. `display_path` carries the real name/extension a
/// dynamic storage's blob-digest `target` no longer has; `inline` renders in place instead of saving.
async fn stream_file(target: &FsPath, display_path: &str, inline: bool) -> Response {
    let metadata = match tokio::fs::metadata(target).await {
        Ok(metadata) => metadata,
        Err(_) => return not_found("no such file"),
    };

    // `GET /{storage}` lists directories; this only ever serves a file.
    if !metadata.is_file() {
        return not_found("no such file");
    }

    let file = match tokio::fs::File::open(target).await {
        Ok(file) => file,
        Err(_) => return not_found("no such file"),
    };

    let name = display_name(display_path);
    let (content_type, disposition) = if inline {
        (
            content_type_for(display_path),
            format!("inline; filename=\"{name}\""),
        )
    } else {
        (
            "application/octet-stream",
            format!("attachment; filename=\"{name}\""),
        )
    };

    Response::streaming(StatusCode::OK, read_stream(file))
        .header("content-type", content_type)
        .header("content-length", metadata.len().to_string())
        .header("content-disposition", disposition)
}

/// The last path segment of `?path=`, cleaned like [`download_name`] for a parseable header.
fn display_name(path: &str) -> String {
    let last = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let cleaned: String = last
        .chars()
        .filter(|c| *c != '"' && *c != '\\' && !c.is_control())
        .collect();
    if cleaned.is_empty() {
        "download".to_string()
    } else {
        cleaned
    }
}

/// `Content-Type` guessed from `path`'s extension, for `?disposition=inline` only.
pub fn content_type_for(path: &str) -> &'static str {
    let ext = path
        .rsplit('.')
        .next()
        .filter(|ext| !ext.contains('/'))
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "oga" | "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" | "aac" => "audio/mp4",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "xml" => "text/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        "txt" | "md" | "markdown" | "log" | "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg"
        | "rs" | "py" | "js" | "ts" | "sh" | "c" | "h" | "cpp" | "go" | "java" | "rb" | "sql"
        | "env" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// The file's own name for a browser saving it; quotes/control bytes dropped, not escaped.
pub fn download_name(target: &FsPath) -> String {
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

/// Reads on a task of its own; `ReceiverStream` is `Sync` regardless of what
/// feeds it, which `Response::streaming` needs and the raw file I/O alone wouldn't satisfy.
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

pub fn register_routes() {
    let _ = handle as fn(_, _, _, _, _) -> _;
    let _ = head as fn(_, _, _, _, _) -> _;
}
