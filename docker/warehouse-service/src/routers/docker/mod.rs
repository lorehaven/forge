use crate::routers::docker_storage_root;
use actix_web::dev::HttpServiceFactory;
use actix_web::web;
use futures_util::StreamExt;
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub mod blob;
pub mod manifest;
pub mod registry;
pub mod token;

/// Hard ceiling on a single blob's assembled size, enforced as bytes stream in
/// (the blob upload handlers read the body as a `Payload` stream, which the
/// global `PayloadConfig` limit does not cover). Generous by default - image
/// layers can legitimately be multiple GB - but bounded so a runaway or
/// malicious upload cannot fill the disk. Override with `MAX_DOCKER_BLOB_BYTES`.
pub fn max_docker_blob_bytes() -> u64 {
    quench_config::ConfigLoader::new("WAREHOUSE")
        .env_u64("MAX_DOCKER_BLOB_BYTES", 32 * 1024 * 1024 * 1024)
}

/// Why streaming a request body onto a blob upload file stopped early.
pub enum AppendError {
    /// The client connection dropped or errored mid-body.
    Read,
    /// Writing to the upload file failed.
    Write,
    /// The assembled upload would exceed [`max_docker_blob_bytes`].
    TooLarge(u64),
}

/// Appends a streamed request body to `file_path` in 64 KiB frames, flushing
/// at the end, and returns the number of bytes written. Never holds more than
/// one frame in memory, so a monolithic multi-GB `PATCH`/`PUT` (what
/// `docker push` and `crane`/`skopeo` send) costs a buffer, not its own size
/// in RAM. `already_on_disk` is the upload file's current length, so the
/// size ceiling is checked against the whole blob, not just this request.
pub async fn append_body_to_upload(
    file_path: &Path,
    already_on_disk: u64,
    body: &mut web::Payload,
) -> Result<u64, AppendError> {
    let limit = max_docker_blob_bytes();
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(file_path)
        .await
        .map_err(|_| AppendError::Write)?;

    let mut written: u64 = 0;
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| AppendError::Read)?;

        written = written.saturating_add(chunk.len() as u64);
        if already_on_disk.saturating_add(written) > limit {
            return Err(AppendError::TooLarge(limit));
        }

        file.write_all(&chunk).await.map_err(|_| AppendError::Write)?;
    }

    file.flush().await.map_err(|_| AppendError::Write)?;
    Ok(written)
}

pub fn upload_path(name: &str, uuid: &str) -> Option<PathBuf> {
    let repo = repository_path(name)?;
    Some(repo.join("_uploads").join(uuid))
}

pub fn blob_path(digest: &str) -> Option<PathBuf> {
    let hex = digest_hex(digest)?;
    Some(
        PathBuf::from(docker_storage_root())
            .join("blobs")
            .join("sha256")
            .join(hex),
    )
}

pub fn manifest_path(digest: &str) -> Option<PathBuf> {
    let hex = digest_hex(digest)?;
    Some(
        PathBuf::from(docker_storage_root())
            .join("manifests")
            .join("sha256")
            .join(hex),
    )
}

pub async fn blob_exists(digest: &str) -> bool {
    let Some(path) = blob_path(digest) else {
        return false;
    };
    tokio::fs::metadata(path).await.is_ok()
}

pub fn validate_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn digest_hex(digest: &str) -> Option<&str> {
    if !validate_digest(digest) {
        return None;
    }
    digest.strip_prefix("sha256:")
}

pub fn repository_path(name: &str) -> Option<PathBuf> {
    if !validate_repository_name(name) {
        return None;
    }
    Some(PathBuf::from(docker_storage_root()).join(name))
}

pub fn validate_repository_name(name: &str) -> bool {
    if name.is_empty() || name.contains('\\') {
        return false;
    }

    Path::new(name)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

pub fn validate_tag_reference(reference: &str) -> bool {
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
