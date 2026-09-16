use crate::domain::docker_error;
use crate::routers::docker::{blob_exists, repository_path, validate_digest};
use quench_http::prelude::{Path, Query, Response, http::StatusCode, post};
use quench_starter::http::domain::error;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct MountQuery {
    pub mount: Option<String>,
    pub from: Option<String>,
}

#[post("/v2/{name:.*}/blobs/uploads/")]
pub async fn handle(Path(name): Path<String>, Query(query): Query<MountQuery>) -> Response {
    if repository_path(&name).is_none() {
        return error::response(
            StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    }

    if let (Some(digest), Some(_)) = (&query.mount, &query.from) {
        if !validate_digest(digest) {
            return error::response(
                StatusCode::BAD_REQUEST,
                error::UNSUPPORTED,
                "invalid digest",
            );
        }

        if blob_exists(digest).await {
            return Response::new(StatusCode::CREATED)
                .header("location", format!("/v2/{}/blobs/{}", name, digest))
                .header("docker-content-digest", digest);
        }
    }

    start_regular_upload(name).await
}

async fn start_regular_upload(name: String) -> Response {
    let uuid = Uuid::new_v4().to_string();

    let Some(repo_path) = repository_path(&name) else {
        return error::response(
            StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    let upload_dir = repo_path.join("_uploads");

    if tokio::fs::create_dir_all(&upload_dir).await.is_err() {
        return error::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    }

    let file_path = upload_dir.join(&uuid);

    if tokio::fs::File::create(&file_path).await.is_err() {
        return error::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    }

    Response::new(StatusCode::ACCEPTED)
        .header("location", format!("/v2/{}/blobs/uploads/{}", name, uuid))
        .header("docker-upload-uuid", &uuid)
        .header("range", "0-0")
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
