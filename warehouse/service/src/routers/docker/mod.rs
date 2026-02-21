use crate::routers::DOCKER_STORAGE_ROOT;
use actix_web::dev::HttpServiceFactory;
use actix_web::web;
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use utoipa::OpenApi;

mod blob;
mod manifest;
pub(crate) mod registry;
pub mod token;

fn upload_path(name: &str, uuid: &str) -> Option<PathBuf> {
    let repo = repository_path(name)?;
    Some(repo.join("_uploads").join(uuid))
}

fn blob_path(digest: &str) -> Option<PathBuf> {
    let hex = digest_hex(digest)?;
    Some(
        PathBuf::from(DOCKER_STORAGE_ROOT.as_str())
            .join("blobs")
            .join("sha256")
            .join(hex),
    )
}

fn manifest_path(digest: &str) -> Option<PathBuf> {
    let hex = digest_hex(digest)?;
    Some(
        PathBuf::from(DOCKER_STORAGE_ROOT.as_str())
            .join("manifests")
            .join("sha256")
            .join(hex),
    )
}

async fn blob_exists(digest: &str) -> bool {
    let Some(path) = blob_path(digest) else {
        return false;
    };
    tokio::fs::metadata(path).await.is_ok()
}

fn validate_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

fn digest_hex(digest: &str) -> Option<&str> {
    if !validate_digest(digest) {
        return None;
    }
    digest.strip_prefix("sha256:")
}

fn repository_path(name: &str) -> Option<PathBuf> {
    if !validate_repository_name(name) {
        return None;
    }
    Some(PathBuf::from(DOCKER_STORAGE_ROOT.as_str()).join(name))
}

fn validate_repository_name(name: &str) -> bool {
    if name.is_empty() || name.contains('\\') {
        return false;
    }

    Path::new(name)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

fn validate_tag_reference(reference: &str) -> bool {
    if reference.is_empty() || reference.contains('\\') {
        return false;
    }

    let mut components = Path::new(reference).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

#[derive(Deserialize)]
pub struct DigestQuery {
    digest: String,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        registry::catalog::handle,
        registry::check::handle_get,
        registry::check::handle_head,
        registry::tags::handle,
        blob::check_exists::handle,
        blob::retrieve::handle,
        blob::get_upload_status::handle,
        blob::cancel_upload::handle,
        blob::complete_upload::handle,
        blob::start_upload::handle,
        blob::upload_chunk::handle,
        manifest::check_exists::handle,
        manifest::get_image::handle,
        manifest::put_image::handle,
        manifest::delete_image::handle,
    ),
    tags((name = "docker", description = "Docker endpoints"))
)]
pub struct DockerApiDoc;

#[derive(OpenApi)]
#[openapi(
    paths(token::handle),
    tags((name = "docker", description = "Auth endpoints"))
)]
pub struct DockerAuthApiDoc;

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/v2")
        // Registry endpoints
        .service(registry::check::handle_get)
        .service(registry::check::handle_head)
        .service(registry::catalog::handle)
        .service(registry::tags::handle)
        // Blob endpoints
        .service(blob::check_exists::handle)
        .service(blob::retrieve::handle)
        .service(blob::get_upload_status::handle)
        .service(blob::cancel_upload::handle)
        .service(blob::complete_upload::handle)
        .service(blob::start_upload::handle)
        .service(blob::upload_chunk::handle)
        // Manifest endpoints
        .service(manifest::check_exists::handle)
        .service(manifest::get_image::handle)
        .service(manifest::put_image::handle)
        .service(manifest::delete_image::handle)
}
