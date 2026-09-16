use crate::domain::docker_error;
use crate::routers::docker::upload_path;
use quench_http::prelude::{Path, Response, get, http::StatusCode};
use quench_starter::http::domain::error;

#[get("/v2/{name:.+}/blobs/uploads/{uuid}")]
pub async fn handle(Path((name, uuid)): Path<(String, String)>) -> Response {
    let Some(upload_path) = upload_path(&name, &uuid) else {
        return error::response(
            StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    let metadata = match tokio::fs::metadata(&upload_path).await {
        Ok(m) => m,
        Err(_) => {
            return error::response(
                StatusCode::NOT_FOUND,
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

    Response::new(StatusCode::NO_CONTENT)
        .header("location", format!("/v2/{name}/blobs/uploads/{uuid}"))
        .header("docker-upload-uuid", &uuid)
        .header("range", range)
        .header("content-length", "0")
}

pub fn register_routes() {
    let _ = handle as fn(_) -> _;
}
