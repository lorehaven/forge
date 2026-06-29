use crate::domain::docker_error;
use crate::routers::docker::{blob_path, validate_digest};
use actix_web::{HttpResponse, Responder, head, web};
use quench_starter::prelude::error;

#[head("/{repo:.*}/blobs/{digest}")]
pub async fn handle(path: web::Path<(String, String)>) -> impl Responder {
    let (_, digest) = path.into_inner();

    // Validate digest format
    if !validate_digest(&digest) {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "invalid digest",
        );
    }

    let Some(blob_path) = blob_path(&digest) else {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
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
        Err(_) => error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob unknown to registry",
        ),
    }
}
