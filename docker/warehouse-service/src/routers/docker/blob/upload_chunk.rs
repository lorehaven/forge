use crate::domain::docker_error;
use crate::routers::docker::upload_path;
use actix_web::{HttpResponse, Responder, patch, web};
use quench_starter::prelude::error;
use tokio::io::AsyncWriteExt;

#[patch("/{name:.*}/blobs/uploads/{uuid}")]
pub async fn handle(path: web::Path<(String, String)>, body: web::Bytes) -> impl Responder {
    let (name, uuid) = path.into_inner();

    let Some(file_path) = upload_path(&name, &uuid) else {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };

    if !file_path.exists() {
        return error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob upload unknown to registry",
        );
    }

    if body.is_empty() {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "empty upload chunk",
        );
    }

    let metadata = match tokio::fs::metadata(&file_path).await {
        Ok(m) => m,
        Err(_) => {
            return error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    let current_size = metadata.len();

    let mut file = match tokio::fs::OpenOptions::new()
        .append(true)
        .open(&file_path)
        .await
    {
        Ok(f) => f,
        Err(_) => {
            return error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "internal server error",
            );
        }
    };

    if file.write_all(&body).await.is_err() {
        return error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    }

    let new_size = current_size + body.len() as u64;

    HttpResponse::Accepted()
        .append_header(("Range", format!("0-{}", new_size - 1)))
        .append_header(("Docker-Upload-UUID", uuid.clone()))
        .append_header(("Location", format!("/v2/{}/blobs/uploads/{}", name, uuid)))
        .finish()
}
