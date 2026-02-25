use crate::domain::docker_error;
use crate::routers::docker::{blob_exists, repository_path, validate_digest};
use actix_web::{HttpResponse, Responder, post, web};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Deserialize, ToSchema)]
pub struct MountQuery {
    pub mount: Option<String>,
    pub from: Option<String>,
}

#[utoipa::path(
    post,
    operation_id = "start_upload",
    tags = ["docker - blob"],
    path = "/{name}/blobs/uploads/",
    params(
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("mount" = Option<String>, Query, description = "Digest to mount"),
        ("from" = Option<String>, Query, description = "Source repository"),
    ),
    responses(
        (
            status = 201,
            description = "Upload completed",
            headers(
                ("Location" = String),
                ("Docker-Content-Digest" = String),
            )
        ),
        (
            status = 202,
            description = "Upload accepted",
            headers(
                ("Location" = String),
                ("Docker-Upload-UUID" = String),
                ("Range" = String),
            )
        ),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Upload session not found"),
        (status = 416, description = "Requested range not satisfiable"),
        (status = 429, description = "Too many requests"),
    )
)]
#[post("/{name:.*}/blobs/uploads/")]
pub async fn handle(path: web::Path<String>, query: web::Query<MountQuery>) -> impl Responder {
    let name = path.into_inner();
    if repository_path(&name).is_none() {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    }

    // Attempt cross-repository mount
    if let (Some(digest), Some(_)) = (&query.mount, &query.from) {
        if !validate_digest(digest) {
            return docker_error::response(
                actix_web::http::StatusCode::BAD_REQUEST,
                docker_error::UNSUPPORTED,
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
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    let upload_dir = repo_path.join("_uploads");

    if tokio::fs::create_dir_all(&upload_dir).await.is_err() {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        );
    }

    let file_path = upload_dir.join(&uuid);

    if tokio::fs::File::create(&file_path).await.is_err() {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        );
    }

    HttpResponse::Accepted()
        .append_header(("Location", format!("/v2/{}/blobs/uploads/{}", name, uuid)))
        .append_header(("Docker-Upload-UUID", uuid))
        .append_header(("Range", "0-0"))
        .finish()
}
