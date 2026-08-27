use crate::domain::docker_error;
use crate::routers::docker::{AppendError, append_body_to_upload, upload_path};
use actix_web::{HttpResponse, Responder, patch, web};
use quench_starter::prelude::error;

#[patch("/{name:.*}/blobs/uploads/{uuid}")]
pub async fn handle(
    path: web::Path<(String, String)>,
    mut body: web::Payload,
) -> impl Responder {
    let (name, uuid) = path.into_inner();

    let Some(file_path) = upload_path(&name, &uuid) else {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };

    let current_size = match tokio::fs::metadata(&file_path).await {
        Ok(m) => m.len(),
        Err(_) => {
            return error::response(
                actix_web::http::StatusCode::NOT_FOUND,
                docker_error::BLOB_UNKNOWN,
                "blob upload unknown to registry",
            );
        }
    };

    // Streamed straight onto the upload file 64 KiB at a time - a monolithic
    // layer PATCH never lands in memory at its own size.
    let written = match append_body_to_upload(&file_path, current_size, &mut body).await {
        Ok(written) => written,
        Err(AppendError::TooLarge(_)) => {
            return error::response(
                actix_web::http::StatusCode::PAYLOAD_TOO_LARGE,
                error::UNSUPPORTED,
                "blob exceeds the maximum allowed size",
            );
        }
        Err(AppendError::Read) => {
            return error::response(
                actix_web::http::StatusCode::BAD_REQUEST,
                error::UNSUPPORTED,
                "the upload was interrupted",
            );
        }
        Err(AppendError::Write) => {
            return error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    if written == 0 {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "empty upload chunk",
        );
    }

    let new_size = current_size + written;

    HttpResponse::Accepted()
        .append_header(("Range", format!("0-{}", new_size - 1)))
        .append_header(("Docker-Upload-UUID", uuid.clone()))
        .append_header(("Location", format!("/v2/{}/blobs/uploads/{}", name, uuid)))
        .finish()
}
