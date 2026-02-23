use crate::domain::docker_error;
use crate::routers::docker::{blob_path, validate_digest};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use std::io::SeekFrom;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

#[utoipa::path(
    get,
    operation_id = "retrieve",
    tags = ["docker"],
    path = "/{name}/blobs/{digest}",
    params(
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("digest" = String, Path, description = "sha256 digest"),
        ("Range" = Option<String>, Header, description = "Optional HTTP range header, e.g. bytes=0-1023")
    ),
    responses(
        (
            status = 200,
            description = "Full blob content returned",
            content(
                ("application/octet-stream")
            ),
            headers(
                ("Docker-Content-Digest" = String, description = "Digest of the blob"),
                ("Content-Length" = u64, description = "Total blob size in bytes"),
                ("Accept-Ranges" = String, description = "Indicates support for byte ranges")
            )
        ),
        (
            status = 206,
            description = "Partial blob content returned",
            content(
                ("application/octet-stream")
            ),
            headers(
                ("Docker-Content-Digest" = String, description = "Digest of the blob"),
                ("Content-Length" = u64, description = "Size of returned range"),
                ("Content-Range" = String, description = "Returned byte range, e.g. bytes 0-1023/2048"),
                ("Accept-Ranges" = String, description = "Indicates support for byte ranges")
            )
        ),
        (
            status = 307,
            description = "Temporary redirect to external blob storage",
            headers(
                ("Location" = String, description = "Redirect target URL")
            )
        ),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Blob not found"),
        (status = 416, description = "Requested range not satisfiable"),
        (status = 429, description = "Too many requests"),
    )
)]
#[get("/{name:.+}/blobs/{digest}")]
pub async fn handle(req: HttpRequest, path: web::Path<(String, String)>) -> impl Responder {
    let (_, digest) = path.into_inner();

    if !validate_digest(&digest) {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::UNSUPPORTED,
            "invalid digest",
        );
    }

    let Some(blob_path) = blob_path(&digest) else {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::UNSUPPORTED,
            "invalid digest",
        );
    };
    if !blob_path.exists() {
        return docker_error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob unknown to registry",
        );
    }

    if let Some(response) = maybe_redirect(&digest) {
        return response;
    }

    serve_with_range(req, blob_path, digest).await
}

fn maybe_redirect(digest: &str) -> Option<HttpResponse> {
    if !envmnt::get_or("ENABLE_REDIRECT", "false")
        .parse::<bool>()
        .unwrap_or(false)
    {
        return None;
    }

    let hex = digest.strip_prefix("sha256:")?;

    let backend_base = envmnt::get_or("BLOB_REDIRECT_BASE", "https://storage.example.com");
    let backend_url = format!("{}/blobs/sha256/{}", backend_base, hex);

    Some(
        HttpResponse::TemporaryRedirect()
            .append_header(("Location", backend_url))
            .finish(),
    )
}

async fn serve_with_range(req: HttpRequest, blob_path: PathBuf, digest: String) -> HttpResponse {
    let file = match File::open(&blob_path).await {
        Ok(f) => f,
        Err(_) => {
            return docker_error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                docker_error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    let metadata = match file.metadata().await {
        Ok(m) => m,
        Err(_) => {
            return docker_error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                docker_error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    let total_size = metadata.len();

    if let Some(range_header) = req.headers().get("Range")
        && let Ok(range_str) = range_header.to_str()
    {
        if let Some((start, end)) = parse_range(range_str, total_size) {
            return serve_partial(file, start, end, total_size, &digest).await;
        }
        return docker_error::response(
            actix_web::http::StatusCode::RANGE_NOT_SATISFIABLE,
            docker_error::UNSUPPORTED,
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
) -> HttpResponse {
    let length = end - start + 1;

    if file.seek(SeekFrom::Start(start)).await.is_err() {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        );
    }

    let mut buffer = vec![0u8; length as usize];
    if file.read_exact(&mut buffer).await.is_err() {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        );
    }

    HttpResponse::PartialContent()
        .append_header(("Content-Type", "application/octet-stream"))
        .append_header((
            "Content-Range",
            format!("bytes {}-{}/{}", start, end, total_size),
        ))
        .append_header(("Content-Length", length))
        .append_header(("Accept-Ranges", "bytes"))
        .append_header(("Docker-Content-Digest", digest))
        .body(buffer)
}

async fn serve_full(mut file: File, total_size: u64, digest: &str) -> HttpResponse {
    let mut buffer = Vec::with_capacity(total_size as usize);

    if file.read_to_end(&mut buffer).await.is_err() {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        );
    }

    HttpResponse::Ok()
        .append_header(("Content-Type", "application/octet-stream"))
        .append_header(("Content-Length", total_size))
        .append_header(("Accept-Ranges", "bytes"))
        .append_header(("Docker-Content-Digest", digest))
        .body(buffer)
}

fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
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
