use crate::domain::docker_error;
use crate::routers::docker::upload_path;
use actix_web::{HttpRequest, HttpResponse, Responder, patch, web};
use tokio::io::AsyncWriteExt;

#[utoipa::path(
    patch,
    operation_id = "upload_chunk",
    tags = ["docker - blob"],
    path = "/{name}/blobs/uploads/{uuid}",
    params(
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("uuid" = String, Path, description = "Upload UUID"),
    ),
    request_body(
        content = String,
        content_type = "application/octet-stream",
        description = "Binary blob chunk",
    ),
    responses(
        (status = 202, description = "Chunk accepted and stored"),
        (status = 400, description = "Malformed content or range"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Upload session not found"),
        (status = 416, description = "Range error"),
        (status = 429, description = "Too many requests"),
    )
)]
#[patch("/{name:.*}/blobs/uploads/{uuid}")]
pub async fn handle(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Bytes,
) -> impl Responder {
    let (name, uuid) = path.into_inner();
    let Some(file_path) = upload_path(&name, &uuid) else {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };

    if !file_path.exists() {
        return docker_error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob upload unknown to registry",
        );
    }

    let metadata = match tokio::fs::metadata(&file_path).await {
        Ok(m) => m,
        Err(_) => {
            return docker_error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                docker_error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    let current_size = metadata.len();

    let content_range = req
        .headers()
        .get("Content-Range")
        .and_then(|v| v.to_str().ok());

    if content_range.is_none() {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::UNSUPPORTED,
            "missing content range",
        );
    }

    if body.is_empty() {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::UNSUPPORTED,
            "empty upload chunk",
        );
    }

    let (start, end) = match parse_content_range(content_range.unwrap()) {
        Some(r) => r,
        None => {
            return docker_error::response(
                actix_web::http::StatusCode::BAD_REQUEST,
                docker_error::UNSUPPORTED,
                "invalid content range",
            );
        }
    };

    let expected_end = start + body.len() as u64 - 1;
    if start != current_size || end != expected_end {
        return docker_error::response(
            actix_web::http::StatusCode::RANGE_NOT_SATISFIABLE,
            docker_error::UNSUPPORTED,
            "invalid content range",
        );
    }

    let mut file = match tokio::fs::OpenOptions::new()
        .append(true)
        .open(&file_path)
        .await
    {
        Ok(f) => f,
        Err(_) => {
            return docker_error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                docker_error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    if file.write_all(&body).await.is_err() {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        );
    }

    let new_size = current_size + body.len() as u64;

    HttpResponse::Accepted()
        .append_header(("Range", format!("0-{}", new_size - 1)))
        .append_header(("Docker-Upload-UUID", uuid))
        .finish()
}

fn parse_content_range(header: &str) -> Option<(u64, u64)> {
    let raw = header.trim();
    let raw = raw
        .strip_prefix("bytes ")
        .or_else(|| raw.strip_prefix("bytes="))
        .unwrap_or(raw);
    let raw = raw.split('/').next()?;

    let parts: Vec<&str> = raw.split('-').collect();
    if parts.len() != 2 {
        return None;
    }
    let start = parts[0].parse().ok()?;
    let end = parts[1].parse().ok()?;
    Some((start, end))
}
