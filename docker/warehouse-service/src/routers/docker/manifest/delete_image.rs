use crate::domain::docker_error;
use crate::routers::docker::{manifest_path, repository_path, validate_digest};
use quench_http::prelude::{Path, Response, delete, http::StatusCode};
use quench_starter::http::domain::error;

#[delete("/v2/{name:.+}/manifests/{reference}")]
pub async fn handle(Path((name, reference)): Path<(String, String)>) -> Response {
    // Must delete by digest only
    if !validate_digest(&reference) {
        return error::response(
            StatusCode::METHOD_NOT_ALLOWED,
            error::UNSUPPORTED,
            "manifest deletion requires a digest reference",
        );
    }

    let Some(repo_path) = repository_path(&name) else {
        return error::response(
            StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };

    let Some(manifest_path) = manifest_path(&reference) else {
        return error::response(
            StatusCode::METHOD_NOT_ALLOWED,
            error::UNSUPPORTED,
            "manifest deletion requires a digest reference",
        );
    };
    if !manifest_path.exists() {
        return error::response(
            StatusCode::NOT_FOUND,
            docker_error::MANIFEST_UNKNOWN,
            "manifest unknown",
        );
    }

    // Remove manifest file
    if tokio::fs::remove_file(&manifest_path).await.is_err() {
        return error::response(
            StatusCode::INTERNAL_SERVER_ERROR,
            error::UNSUPPORTED,
            "internal server error",
        );
    }

    // Optional: remove tag references pointing to this digest
    let tags_dir = repo_path.join("tags");
    if let Ok(mut entries) = tokio::fs::read_dir(&tags_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await
            && let Ok(content) = tokio::fs::read_to_string(entry.path()).await
        {
            if content.trim() == reference {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
    }

    Response::new(StatusCode::ACCEPTED)
}

pub fn register_routes() {
    let _ = handle as fn(_) -> _;
}
