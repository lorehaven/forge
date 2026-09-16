use crate::domain::docker_error;
use crate::routers::docker::{blob_path, validate_digest};
use async_trait::async_trait;
use quench_http::prelude::{
    FromRequest, HttpError, Path, Request, Response, get, http::StatusCode,
};
use quench_starter::http::domain::error;
use std::{io::SeekFrom, path::PathBuf};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

/// The raw `Range` header, if any.
pub struct RangeHeader(pub Option<String>);

#[async_trait]
impl FromRequest for RangeHeader {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(req.header("range").map(str::to_string)))
    }
}

#[get("/v2/{name:.+}/blobs/{digest}")]
pub async fn handle(
    RangeHeader(range): RangeHeader,
    Path((_name, digest)): Path<(String, String)>,
) -> Response {
    if !validate_digest(&digest) {
        return error::response(
            StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "invalid digest",
        );
    }

    let Some(blob_path) = blob_path(&digest) else {
        return error::response(
            StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "invalid digest",
        );
    };
    if !blob_path.exists() {
        return error::response(
            StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob unknown to registry",
        );
    }

    if let Some(response) = maybe_redirect(&digest) {
        return response;
    }

    serve_with_range(range, blob_path, digest).await
}

pub fn maybe_redirect(digest: &str) -> Option<Response> {
    if !envmnt::get_or("ENABLE_REDIRECT", "false")
        .parse::<bool>()
        .unwrap_or(false)
    {
        return None;
    }

    let hex = digest.strip_prefix("sha256:")?;

    let backend_base = envmnt::get_or("BLOB_REDIRECT_BASE", "https://storage.example.com");
    let backend_url = format!("{}/blobs/sha256/{}", backend_base, hex);

    Some(Response::new(StatusCode::TEMPORARY_REDIRECT).header("location", backend_url))
}

async fn serve_with_range(range: Option<String>, blob_path: PathBuf, digest: String) -> Response {
    let file = match File::open(&blob_path).await {
        Ok(f) => f,
        Err(_) => {
            return error::response(
                StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    let metadata = match file.metadata().await {
        Ok(m) => m,
        Err(_) => {
            return error::response(
                StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    let total_size = metadata.len();

    if let Some(range_str) = range {
        if let Some((start, end)) = parse_range(&range_str, total_size) {
            return serve_partial(file, start, end, total_size, &digest).await;
        }
        return error::response(
            StatusCode::RANGE_NOT_SATISFIABLE,
            error::UNSUPPORTED,
            "requested range not satisfiable",
        );
    }

    serve_full(file, total_size, &digest).await
}

async fn serve_partial(
    mut file: File,
    start: u64,
    end: u64,
    total_size: u64,
    digest: &str,
) -> Response {
    let length = end - start + 1;

    if file.seek(SeekFrom::Start(start)).await.is_err() {
        return error::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    }

    let stream = ReaderStream::new(file.take(length));

    Response::streaming(StatusCode::PARTIAL_CONTENT, stream)
        .header("content-type", "application/octet-stream")
        .header(
            "content-range",
            format!("bytes {}-{}/{}", start, end, total_size),
        )
        .header("content-length", length.to_string())
        .header("accept-ranges", "bytes")
        .header("docker-content-digest", digest)
}

async fn serve_full(file: File, total_size: u64, digest: &str) -> Response {
    let stream = ReaderStream::new(file);

    Response::streaming(StatusCode::OK, stream)
        .header("content-type", "application/octet-stream")
        .header("content-length", total_size.to_string())
        .header("accept-ranges", "bytes")
        .header("docker-content-digest", digest)
}

pub fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
    if !header.starts_with("bytes=") {
        return None;
    }

    let parts: Vec<&str> = header[6..].split('-').collect();
    if parts.len() != 2 {
        return None;
    }

    let start: u64 = parts[0].parse().ok()?;
    let end: u64 = if parts[1].is_empty() {
        total - 1
    } else {
        parts[1].parse().ok()?
    };

    if start > end || end >= total {
        return None;
    }

    Some((start, end))
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
