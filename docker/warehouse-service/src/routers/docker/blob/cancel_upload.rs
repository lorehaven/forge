use crate::domain::docker_error;
use crate::routers::docker::upload_path;
use quench_http::prelude::{Path, Response, delete, http::StatusCode};
use quench_starter::http::domain::error;

#[delete("/v2/{name:.+}/blobs/uploads/{uuid}")]
pub async fn handle(Path((name, uuid)): Path<(String, String)>) -> Response {
    let Some(upload_path) = upload_path(&name, &uuid) else {
        return error::response(
            StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    if !upload_path.exists() {
        return error::response(
            StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob upload unknown to registry",
        );
    }

    match tokio::fs::remove_file(&upload_path).await {
        Ok(_) => Response::new(StatusCode::NO_CONTENT),
        Err(_) => error::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        ),
    }
}

pub fn register_routes() {
    let _ = handle as fn(_) -> _;
}
