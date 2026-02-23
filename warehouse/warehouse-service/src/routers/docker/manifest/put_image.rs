use crate::domain::docker_error;
use crate::routers::docker::{
    manifest_path, repository_path, validate_digest, validate_tag_reference,
};
use actix_web::{HttpRequest, HttpResponse, Responder, put, web};

const DOCKER_MANIFEST_V2: &str = "application/vnd.docker.distribution.manifest.v2+json";
const DOCKER_MANIFEST_LIST_V2: &str = "application/vnd.docker.distribution.manifest.list.v2+json";
const OCI_IMAGE_MANIFEST_V1: &str = "application/vnd.oci.image.manifest.v1+json";
const OCI_IMAGE_INDEX_V1: &str = "application/vnd.oci.image.index.v1+json";

#[utoipa::path(
    put,
    operation_id = "put_image",
    tags = ["docker"],
    path = "/{name}/manifests/{reference}",
    params(
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("reference" = String, Path, description = "Tag or digest of the target manifest"),
    ),
    request_body(
        content = String,
        content(
            ("application/vnd.docker.distribution.manifest.v2+json"),
            ("application/vnd.oci.image.manifest.v1+json"),
            ("application/vnd.docker.distribution.manifest.list.v2+json"),
            ("application/vnd.oci.image.index.v1+json")
        ),
        description = "Docker/OCI manifest payload",
    ),
    responses(
        (status = 201, description = "Manifest created successfully"),
        (status = 400, description = "Invalid name, reference, or manifest."),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Repository not found"),
        (status = 405, description = "Operation not allowed"),
        (status = 429, description = "Too many requests"),
    )
)]
#[put("/{name:.+}/manifests/{reference}")]
pub async fn handle(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Bytes,
) -> impl Responder {
    let (name, reference) = path.into_inner();

    use sha2::{Digest, Sha256};

    let content_type = req
        .headers()
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .and_then(normalize_media_type);

    if let Some(ct) = content_type
        && !is_supported_manifest_media_type(ct)
    {
        return docker_error::response(
            actix_web::http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
            docker_error::UNSUPPORTED,
            "manifest media type unsupported",
        );
    }

    // Compute manifest digest
    let mut hasher = Sha256::new();
    hasher.update(&body);
    let digest = format!("sha256:{:x}", hasher.finalize());

    let Some(repo_path) = repository_path(&name) else {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };

    if tokio::fs::create_dir_all(&repo_path).await.is_err() {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        );
    }

    // Save manifest by digest
    let Some(manifest_path) = manifest_path(&digest) else {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        );
    };
    if let Some(parent) = manifest_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    if tokio::fs::write(&manifest_path, &body).await.is_err() {
        return docker_error::response(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            docker_error::UNSUPPORTED,
            "internal server error",
        );
    }

    // Save tag reference only when reference is a tag (not a digest).
    if !validate_digest(&reference) {
        if !validate_tag_reference(&reference) {
            return docker_error::response(
                actix_web::http::StatusCode::BAD_REQUEST,
                docker_error::UNSUPPORTED,
                "invalid manifest reference",
            );
        }
        let tag_path = repo_path.join("tags").join(&reference);

        if let Some(parent) = tag_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        let _ = tokio::fs::write(&tag_path, digest.as_bytes()).await;
    }

    HttpResponse::Created()
        .append_header(("Location", format!("/v2/{name}/manifests/{reference}")))
        .append_header(("Docker-Content-Digest", digest))
        .finish()
}

fn normalize_media_type(raw: &str) -> Option<&str> {
    let value = raw.split(';').next()?.trim();
    if value.is_empty() { None } else { Some(value) }
}

fn is_supported_manifest_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        DOCKER_MANIFEST_V2 | DOCKER_MANIFEST_LIST_V2 | OCI_IMAGE_MANIFEST_V1 | OCI_IMAGE_INDEX_V1
    )
}
