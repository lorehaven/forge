use crate::domain::docker_error;
use crate::routers::docker::upload_path;
use actix_web::{HttpResponse, Responder, get, web};

#[utoipa::path(
    get,
    operation_id = "get_upload_status",
    tags = ["docker"],
    path = "/{name}/blobs/uploads/{uuid}",
    params(
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("uuid" = String, Path, description = "Upload UUID"),
    ),
    responses(
        (status = 204, description = "Upload in progress. No body is returned."),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Upload session not found"),
        (status = 429, description = "Too many requests"),
    )
)]
#[get("/{name:.+}/blobs/uploads/{uuid}")]
pub async fn handle(path: web::Path<(String, String)>) -> impl Responder {
    let (name, uuid) = path.into_inner();

    let Some(upload_path) = upload_path(&name, &uuid) else {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    let metadata = match tokio::fs::metadata(&upload_path).await {
        Ok(m) => m,
        Err(_) => {
            return docker_error::response(
                actix_web::http::StatusCode::NOT_FOUND,
                docker_error::BLOB_UNKNOWN,
                "blob upload unknown to registry",
            );
        }
    };

    let size = metadata.len();
    let range = if size == 0 {
        "0-0".to_string()
    } else {
        format!("0-{}", size - 1)
    };

    HttpResponse::NoContent()
        .append_header(("Location", format!("/v2/{name}/blobs/uploads/{uuid}")))
        .append_header(("Docker-Upload-UUID", uuid))
        .append_header(("Range", range))
        .append_header(("Content-Length", 0))
        .finish()
}
