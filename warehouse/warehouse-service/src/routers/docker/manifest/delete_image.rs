use crate::domain::docker_error;
use crate::routers::docker::{manifest_path, repository_path, validate_digest};
use actix_web::{HttpResponse, Responder, delete, web};

#[utoipa::path(
    delete,
    operation_id = "delete_image",
    tags = ["docker - manifest"],
    path = "/{name}/manifests/{reference}",
    params(
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("reference" = String, Path, description = "Tag or digest of the target manifest"),
    ),
    responses(
        (status = 202, description = "Manifest deleted successfully. No content returned."),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied. The manifest may still be referenced."),
        (status = 404, description = "Manifest or repository not found"),
        (status = 405, description = "Only digest-based deletion is allowed."),
        (status = 429, description = "Too many requests"),
    )
)]
#[delete("/{name:.+}/manifests/{reference}")]
pub async fn handle(path: web::Path<(String, String)>) -> impl Responder {
    let (name, reference) = path.into_inner();

    // Must delete by digest only
    if !validate_digest(&reference) {
        return docker_error::response(
            actix_web::http::StatusCode::METHOD_NOT_ALLOWED,
            docker_error::UNSUPPORTED,
            "manifest deletion requires a digest reference",
        );
    }

    let Some(repo_path) = repository_path(&name) else {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };

    let Some(manifest_path) = manifest_path(&reference) else {
        return docker_error::response(
            actix_web::http::StatusCode::METHOD_NOT_ALLOWED,
            docker_error::UNSUPPORTED,
            "manifest deletion requires a digest reference",
        );
    };
    if !manifest_path.exists() {
        return docker_error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::MANIFEST_UNKNOWN,
            "manifest unknown",
        );
    }

    // Remove manifest file
    if tokio::fs::remove_file(&manifest_path).await.is_err() {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
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

    HttpResponse::Accepted().finish()
}
