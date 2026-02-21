use crate::routers::docker::upload_path;
use crate::shared::docker_error;
use actix_web::{HttpResponse, Responder, delete, web};

#[utoipa::path(
    delete,
    operation_id = "cancel_upload",
    tags = ["docker"],
    path = "/{name}/blobs/uploads/{uuid}",
    params(
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("uuid" = String, Path, description = "Upload UUID"),
    ),
    responses(
        (status = 204, description = "Upload session cancelled successfully. No body is returned."),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Upload session not found"),
        (status = 429, description = "Too many requests"),
    )
)]
#[delete("/{name:.+}/blobs/uploads/{uuid}")]
pub async fn handle(path: web::Path<(String, String)>) -> impl Responder {
    let (name, uuid) = path.into_inner();

    let Some(upload_path) = upload_path(&name, &uuid) else {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    if !upload_path.exists() {
        return docker_error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob upload unknown to registry",
        );
    }

    match tokio::fs::remove_file(&upload_path).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(_) => docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        ),
    }
}
