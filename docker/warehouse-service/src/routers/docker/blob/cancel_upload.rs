use crate::domain::docker_error;
use crate::routers::docker::upload_path;
use actix_web::{HttpResponse, Responder, delete, web};
use quench_starter::prelude::error;

#[delete("/{name:.+}/blobs/uploads/{uuid}")]
pub async fn handle(path: web::Path<(String, String)>) -> impl Responder {
    let (name, uuid) = path.into_inner();

    let Some(upload_path) = upload_path(&name, &uuid) else {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    if !upload_path.exists() {
        return error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob upload unknown to registry",
        );
    }

    match tokio::fs::remove_file(&upload_path).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(_) => error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        ),
    }
}
