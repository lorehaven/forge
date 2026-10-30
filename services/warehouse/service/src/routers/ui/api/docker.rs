//! Docker repositories and tags, and removing an image.

use super::{Session, error, internal, no_content, ok};
use crate::routers::docker::registry::storage::{
    TagListError, list_repositories, list_tag_metadata_for_repository,
};
use crate::routers::docker::{manifest_path, repository_path, validate_digest};
use quench_http::prelude::{Json, Query, Response, get, http::StatusCode, post};
use quench_serde::Deserialize;
use warehouse_core::{DockerTag, ImageRef};

#[get("/ui/api/docker/repositories")]
pub async fn repositories(session: Session) -> Response {
    if let Err(response) = session.require() {
        return response;
    }
    ok(&list_repositories())
}

#[derive(Debug, Deserialize)]
pub struct RepoQuery {
    pub repo: String,
}

/// A repository's tags with their digests, newest first. 404 for a repository that does not exist.
#[get("/ui/api/docker/tags")]
pub async fn tags(session: Session, Query(query): Query<RepoQuery>) -> Response {
    if let Err(response) = session.require() {
        return response;
    }
    match list_tag_metadata_for_repository(&query.repo) {
        Ok(items) => ok(&items
            .into_iter()
            .map(|meta| DockerTag {
                tag: meta.tag,
                digest: meta.digest,
                media_type: meta.media_type,
                size_bytes: meta.size_bytes,
            })
            .collect::<Vec<_>>()),
        Err(TagListError::InvalidName) => {
            error(StatusCode::BAD_REQUEST, "api_error_invalid_repository")
        }
        Err(TagListError::NotFound) => error(StatusCode::NOT_FOUND, "api_error_invalid_repository"),
    }
}

/// Deletes the manifest the digest names and every tag of the repository that points at it. As before, any session may.
#[post("/ui/api/docker/delete-image")]
pub async fn delete_image(session: Session, Json(body): Json<ImageRef>) -> Response {
    if let Err(response) = session.require() {
        return response;
    }
    if !validate_digest(&body.digest) {
        return error(StatusCode::BAD_REQUEST, "api_error_digest_required");
    }
    let Some(repo_path) = repository_path(&body.repository) else {
        return error(StatusCode::BAD_REQUEST, "api_error_invalid_repository");
    };
    let Some(manifest_path) = manifest_path(&body.digest) else {
        return error(StatusCode::BAD_REQUEST, "api_error_invalid_digest");
    };
    if !manifest_path.exists() {
        return error(StatusCode::NOT_FOUND, "api_error_manifest_unknown");
    }
    if let Err(err) = tokio::fs::remove_file(&manifest_path).await {
        quench_log::error!("Failed to delete manifest: {}", err);
        return internal();
    }

    let tags_dir = repo_path.join("tags");
    if let Ok(mut entries) = tokio::fs::read_dir(&tags_dir).await {
        // Every tag, not just the first run of matches: a tag of another image does not end the walk.
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(content) = tokio::fs::read_to_string(entry.path()).await
                && content.trim() == body.digest
            {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
    }
    no_content()
}

pub fn register_routes() {
    let _ = repositories as fn(_) -> _;
    let _ = tags as fn(_, _) -> _;
    let _ = delete_image as fn(_, _) -> _;
}
