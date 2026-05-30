use crate::domain::docker_error;
use crate::routers::docker::{blob_exists, repository_path, validate_digest};
use actix_web::{HttpResponse, Responder, post, web};
use quench_srv::prelude::error;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct MountQuery {
    pub mount: Option<String>,
    pub from: Option<String>,
}

#[post("/{name:.*}/blobs/uploads/")]
pub async fn handle(path: web::Path<String>, query: web::Query<MountQuery>) -> impl Responder {
    let name = path.into_inner();
    if repository_path(&name).is_none() {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    }

    // Attempt cross-repository mount
    if let (Some(digest), Some(_)) = (&query.mount, &query.from) {
        if !validate_digest(digest) {
            return error::response(
                actix_web::http::StatusCode::BAD_REQUEST,
                error::UNSUPPORTED,
                "invalid digest",
            );
        }

        if blob_exists(digest).await {
            return HttpResponse::Created()
                .append_header(("Location", format!("/v2/{}/blobs/{}", name, digest)))
                .append_header(("Docker-Content-Digest", digest.clone()))
                .finish();
        }
    }

    start_regular_upload(name).await
}

async fn start_regular_upload(name: String) -> HttpResponse {
    let uuid = Uuid::new_v4().to_string();

    let Some(repo_path) = repository_path(&name) else {
        return error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    let upload_dir = repo_path.join("_uploads");

    if tokio::fs::create_dir_all(&upload_dir).await.is_err() {
        return error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    }

    let file_path = upload_dir.join(&uuid);

    if tokio::fs::File::create(&file_path).await.is_err() {
        return error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    }

    HttpResponse::Accepted()
        .append_header(("Location", format!("/v2/{}/blobs/uploads/{}", name, uuid)))
        .append_header(("Docker-Upload-UUID", uuid))
        .append_header(("Range", "0-0"))
        .finish()
}
