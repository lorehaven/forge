use crate::routers::docker::{blob_path, validate_digest};
use crate::shared::docker_error;
use actix_web::{HttpResponse, Responder, head, web};

#[utoipa::path(
    head,
    operation_id = "check_exists",
    tags = ["docker"],
    path = "/{name}/blobs/{digest}",
    params(
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("digest" = String, Path, description = "sha256 digest"),
    ),
    responses(
        (
            status = 200,
            description = "Blob exists",
            headers(
                ("Docker-Content-Digest" = String),
                ("Content-Length" = u64),
            )
        ),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Blob not found"),
        (status = 429, description = "Too many requests"),
    )
)]
#[head("/{repo:.*}/blobs/{digest}")]
pub async fn handle(path: web::Path<(String, String)>) -> impl Responder {
    let (_, digest) = path.into_inner();

    // Validate digest format
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
    match std::fs::metadata(&blob_path) {
        Ok(metadata) => HttpResponse::Ok()
            .append_header(("Content-Type", "application/octet-stream"))
            .append_header(("Docker-Content-Digest", digest))
            .append_header(("Content-Length", metadata.len()))
            .append_header(("Accept-Ranges", "bytes"))
            .finish(),
        Err(_) => docker_error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob unknown to registry",
        ),
    }
}
