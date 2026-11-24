//! `GET`/`HEAD /api/v1/files/{storage}/file?path=…` - streamed off disk in
//! chunks, so serving (and concurrently downloading) a large file stays cheap in memory.

use super::{
    ResolvedStorage, authorize, dynamic_path, error, forbidden, not_found, resolve_storage,
};
use crate::domain::storage_file;
use crate::routers::files::{FetchHeaders, FileQuery, OptionalClaims, dynamic};
use quench_auth::domain::jwt::JwtConfig;
use quench_core::async_trait;
use quench_core::bytes::Bytes;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Endpoint, FromRequest, Inject, Path, Query, Request, Response, get,
    http::{Method, StatusCode},
};
use std::path::{Path as FsPath, PathBuf};
use tokio::io::AsyncReadExt;

/// How much is read from disk, and handed to the network, at a time.
const CHUNK_BYTES: usize = 64 * 1024;

/// How many chunks may sit between the reader and a slow client - small so a
/// stalled client stalls the reader instead of buffering the whole file.
const CHUNK_BUFFER: usize = 8;

#[get("/api/v1/files/{storage}/file")]
#[quench_log::instrument(skip(claims, config, db))]
pub async fn handle(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Path(storage_name): Path<String>,
    Query(query): Query<FileQuery>,
    headers: FetchHeaders,
) -> Response {
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };

    if !authorize(claims.as_ref(), &config, &resolved, "read") {
        return forbidden("read access to this storage is required");
    }

    // A dynamic file is its digest: the validator costs nothing and is exact.
    let mut digest: Option<String> = None;
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
                Ok(Some((sha256, _size))) => {
                    digest = Some(sha256.clone());
                    let blob = dynamic::blob_path(&root, &sha256);
                    // The row is there but its bytes are not on disk: say so, rather than "no such file".
                    if !tokio::fs::try_exists(&blob).await.unwrap_or(false) {
                        quench_log::error!(
                            "blob {sha256} of `{}` in `{}` is missing on disk",
                            query.path,
                            storage.name
                        );
                        return not_found("the content of this file is missing on the server");
                    }
                    blob
                }
                Ok(None) => return not_found("no such file"),
                Err(problem) => {
                    quench_log::error!("dynamic download lookup failed: {problem}");
                    return error(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed");
                }
            }
        }
    };

    let inline = query.disposition.as_deref() == Some("inline");
    let options = StreamOptions {
        digest,
        headers,
        thumb: query.thumb.filter(|_| inline),
    };
    stream_file_with(&target, &query.path, inline, options).await
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
                    quench_log::error!("dynamic download lookup failed: {problem}");
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

quench_http::submit!(
    quench_http::route::ROUTES,
    quench_http::route::RouteRegistration,
    quench_http::route::RouteRegistration {
        method: Method::HEAD,
        pattern: "/api/v1/files/{storage}/file",
        endpoint: || std::sync::Arc::new(__quench_route_head) as std::sync::Arc<dyn Endpoint>,
    }
);

/// What a download knows beyond the file: its digest (if it has one), the request's headers, a thumbnail size.
#[derive(Debug, Default)]
pub struct StreamOptions {
    pub digest: Option<String>,
    pub headers: FetchHeaders,
    pub thumb: Option<u32>,
}

pub(crate) async fn stream_file(target: &FsPath, display_path: &str, inline: bool) -> Response {
    stream_file_with(target, display_path, inline, StreamOptions::default()).await
}

/// The first `bytes=a-b` range of a `Range` header, resolved against a file of `len` bytes.
/// `Err(())` is a range that cannot be satisfied; `Ok(None)` is no (or an unreadable) range: send it all.
#[allow(
    clippy::result_unit_err,
    reason = "the only failure is 'not satisfiable'"
)]
pub fn parse_range(header: Option<&str>, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = header.and_then(|h| h.trim().strip_prefix("bytes=")) else {
        return Ok(None);
    };
    let first = spec.split(',').next().unwrap_or("").trim();
    let Some((from, to)) = first.split_once('-') else {
        return Ok(None);
    };
    let (start, end) = if from.is_empty() {
        // `-N`: the last N bytes.
        let Ok(n) = to.parse::<u64>() else {
            return Ok(None);
        };
        if n == 0 || len == 0 {
            return Err(());
        }
        (len.saturating_sub(n), len - 1)
    } else {
        let Ok(start) = from.parse::<u64>() else {
            return Ok(None);
        };
        let end = if to.is_empty() {
            len.saturating_sub(1)
        } else {
            match to.parse::<u64>() {
                Ok(end) => end.min(len.saturating_sub(1)),
                Err(_) => return Ok(None),
            }
        };
        (start, end)
    };
    if start >= len || start > end {
        return Err(());
    }
    Ok(Some((start, end)))
}

fn validator(digest: Option<&str>, metadata: &std::fs::Metadata) -> String {
    if let Some(digest) = digest {
        return format!("\"{digest}\"");
    }
    let modified = metadata
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    format!("\"{modified:x}-{:x}\"", metadata.len())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Thumbnails being made at once: decoding a photo is a few hundred MB of RGBA, so a folder of them must queue.
static THUMB_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

fn thumb_dir() -> PathBuf {
    dynamic::root().map_or_else(
        || std::env::temp_dir().join("warehouse-thumbs"),
        |root| root.join(".thumbs"),
    )
}

/// A PNG of `source` that fits in `side` pixels, made once and kept on disk under the file's validator.
/// `None` when the file is not an image this can read (the caller sends the original).
async fn thumbnail(source: &FsPath, key: &str, side: u32) -> Option<PathBuf> {
    let cached = thumb_dir().join(format!(
        "{}-{side}.png",
        hex(&quench_crypto::hash::sha256(key))
    ));
    if tokio::fs::try_exists(&cached).await.unwrap_or(false) {
        return Some(cached);
    }
    let _slot = THUMB_SLOTS.acquire().await.ok()?;
    // Another request may have made it while this one queued.
    if tokio::fs::try_exists(&cached).await.unwrap_or(false) {
        return Some(cached);
    }
    let bytes = tokio::fs::read(source).await.ok()?;
    let png = tokio::task::spawn_blocking(move || {
        let image = quench_image::decode(&bytes).ok()?;
        drop(bytes);
        Some(shrink_to_png(&image, side))
    })
    .await
    .ok()??;
    tokio::fs::create_dir_all(cached.parent()?).await.ok()?;
    // Written beside and renamed, so a half-written thumbnail is never served.
    let part = cached.with_extension("part");
    tokio::fs::write(&part, &png).await.ok()?;
    tokio::fs::rename(&part, &cached).await.ok()?;
    Some(cached)
}

/// `image` scaled down (never up) to fit `side` x `side`, averaging the source pixels under each new one.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "pixel arithmetic bounded by the decoder's 16384-pixel limit"
)]
fn shrink_to_png(image: &quench_image::Image, side: u32) -> Vec<u8> {
    let (width, height) = (image.width.max(1), image.height.max(1));
    let scale = (f64::from(side) / f64::from(width.max(height))).min(1.0);
    let new_width = ((f64::from(width) * scale).round() as u32).max(1);
    let new_height = ((f64::from(height) * scale).round() as u32).max(1);
    // The source span [from, to) that new pixel `index` of `count` covers in `size`.
    let span = |index: u32, count: u32, size: u32| {
        let from = (u64::from(index) * u64::from(size) / u64::from(count)) as u32;
        let to = ((u64::from(index + 1) * u64::from(size) / u64::from(count)) as u32)
            .clamp(from + 1, size);
        (from, to)
    };
    let mut out = vec![0u8; new_width as usize * new_height as usize * 4];
    for row in 0..new_height {
        let (top, bottom) = span(row, new_height, height);
        for col in 0..new_width {
            let (left, right) = span(col, new_width, width);
            // Alpha-weighted sums, so a transparent pixel does not tint its neighbours.
            let (mut red, mut green, mut blue, mut alpha_sum, mut count) =
                (0u64, 0u64, 0u64, 0u64, 0u64);
            for sy in top..bottom {
                let line = sy as usize * width as usize;
                for sx in left..right {
                    let at = (line + sx as usize) * 4;
                    let alpha = u64::from(image.rgba[at + 3]);
                    red += u64::from(image.rgba[at]) * alpha;
                    green += u64::from(image.rgba[at + 1]) * alpha;
                    blue += u64::from(image.rgba[at + 2]) * alpha;
                    alpha_sum += alpha;
                    count += 1;
                }
            }
            let to = (row as usize * new_width as usize + col as usize) * 4;
            if let (Some(r), Some(g), Some(b)) = (
                red.checked_div(alpha_sum),
                green.checked_div(alpha_sum),
                blue.checked_div(alpha_sum),
            ) {
                out[to] = r as u8;
                out[to + 1] = g as u8;
                out[to + 2] = b as u8;
                out[to + 3] = alpha_sum.checked_div(count).unwrap_or(0) as u8;
            }
        }
    }
    quench_image::encode_png(new_width, new_height, &out)
}

/// Streams `target` back, whole or the one range asked for, with a validator so a browser reuses what it has.
///
/// `display_path` carries the real name/extension a dynamic storage's blob-digest `target` no longer has;
/// `inline` renders in place instead of saving.
pub async fn stream_file_with(
    target: &FsPath,
    display_path: &str,
    inline: bool,
    options: StreamOptions,
) -> Response {
    let Ok(metadata) = tokio::fs::metadata(target).await else {
        return not_found("no such file");
    };

    // `GET /{storage}` lists directories; this only ever serves a file.
    if !metadata.is_file() {
        return not_found("no such file");
    }

    let etag = validator(options.digest.as_deref(), &metadata);
    // Private: the bytes are behind a login. `no-cache` = keep it, but ask (a 304 costs nothing) before using it.
    let cache = "private, no-cache";
    let name = display_name(display_path);

    // A thumbnail is a different body of the same file: its own etag, no ranges, a PNG.
    let thumb_etag = options
        .thumb
        .map(|side| format!("\"{}-t{side}\"", etag.trim_matches('"')));
    let sent_etag = thumb_etag.as_deref().unwrap_or(&etag);

    if let Some(seen) = options.headers.if_none_match.as_deref()
        && seen
            .split(',')
            .any(|t| t.trim() == sent_etag || t.trim() == "*")
    {
        return Response::new(StatusCode::NOT_MODIFIED)
            .header("etag", sent_etag)
            .header("cache-control", cache);
    }

    if let Some(side) = options.thumb {
        let side = side.clamp(32, 2048);
        let key = format!("{etag}|{display_path}");
        if let Some(small) = thumbnail(target, &key, side).await
            && let Ok(file) = tokio::fs::File::open(&small).await
            && let Ok(small_meta) = file.metadata().await
        {
            return Response::streaming(StatusCode::OK, read_stream(file, 0, small_meta.len()))
                .header("content-type", "image/png")
                .header("content-length", small_meta.len().to_string())
                .header("etag", sent_etag)
                .header("cache-control", cache)
                .header(
                    "content-disposition",
                    format!("inline; filename=\"{name}\""),
                );
        }
        // Not an image it can read: the original, below, under its own etag.
    }

    let len = metadata.len();
    let (status, start, end) = match parse_range(options.headers.range.as_deref(), len) {
        Ok(Some((start, end))) => (StatusCode::PARTIAL_CONTENT, start, end),
        Ok(None) => (StatusCode::OK, 0, len.saturating_sub(1)),
        Err(()) => {
            return Response::new(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("content-range", format!("bytes */{len}"));
        }
    };
    let count = if len == 0 { 0 } else { end - start + 1 };

    let Ok(file) = tokio::fs::File::open(target).await else {
        return not_found("no such file");
    };

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

    let mut response = Response::streaming(status, read_stream(file, start, count))
        .header("content-type", content_type)
        .header("content-length", count.to_string())
        .header("content-disposition", disposition)
        .header("accept-ranges", "bytes")
        .header("etag", &etag)
        .header("cache-control", cache);
    if status == StatusCode::PARTIAL_CONTENT {
        response = response.header("content-range", format!("bytes {start}-{end}/{len}"));
    }
    response
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
#[must_use]
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
#[must_use]
pub fn download_name(target: &FsPath) -> String {
    let name = target.file_name().map_or_else(
        || "download".to_string(),
        |name| name.to_string_lossy().to_string(),
    );

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
fn read_stream(
    mut file: tokio::fs::File,
    start: u64,
    count: u64,
) -> impl quench_core::stream::Stream<Item = Result<Bytes, std::io::Error>> + Send + Unpin + 'static
{
    let (sender, receiver) = tokio::sync::mpsc::channel(CHUNK_BUFFER);

    tokio::spawn(async move {
        use tokio::io::AsyncSeekExt;
        if start > 0 && file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
            return;
        }
        let mut left = count;
        let mut buffer = vec![0u8; CHUNK_BYTES];
        while left > 0 {
            let want = usize::try_from(left).map_or(CHUNK_BYTES, |l| l.min(CHUNK_BYTES));
            match file.read(&mut buffer[..want]).await {
                Ok(0) => break,
                Ok(read) => {
                    left -= read as u64;
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

    quench_http::stream::receiver_stream(receiver)
}

/// Kept for the delete handler, which wants the same "is this a file" answer.
pub async fn is_file(target: &PathBuf) -> bool {
    tokio::fs::metadata(target)
        .await
        .is_ok_and(|metadata| metadata.is_file())
}

pub fn register_routes() {
    let _ = handle as fn(_, _, _, _, _, _) -> _;
    let _ = head as fn(_, _, _, _, _) -> _;
}
