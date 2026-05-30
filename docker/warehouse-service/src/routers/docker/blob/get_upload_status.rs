use crate::domain::docker_error;
use crate::routers::docker::upload_path;
use actix_web::{HttpResponse, Responder, get, web};
use quench_srv::prelude::error;

#[get("/{name:.+}/blobs/uploads/{uuid}")]
pub async fn handle(path: web::Path<(String, String)>) -> impl Responder {
    let (name, uuid) = path.into_inner();

    let Some(upload_path) = upload_path(&name, &uuid) else {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    let metadata = match tokio::fs::metadata(&upload_path).await {
        Ok(m) => m,
        Err(_) => {
            return error::response(
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
