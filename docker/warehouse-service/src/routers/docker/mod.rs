use crate::routers::docker_storage_root;
use async_trait::async_trait;
use futures_util::StreamExt;
use http_body_util::BodyExt;
use quench_http::body::InboundBody;
use quench_http::prelude::{FromRequest, HttpError, Request};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub mod blob;
pub mod manifest;
pub mod registry;
pub mod token;

/// The unread request body - blobs can be multiple GB, too large for the buffered extractors.
pub struct RawBody(pub InboundBody);

#[async_trait]
impl FromRequest for RawBody {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(req.take_body()))
    }
}

/// Hard ceiling on a single blob's assembled size, enforced as bytes stream in.
/// Override with `MAX_DOCKER_BLOB_BYTES`.
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

/// Streams `body` onto `file_path` in frames (never buffering the whole thing), returning bytes
/// written. `already_on_disk` lets the size ceiling apply to the whole blob, not just this request.
pub async fn append_body_to_upload(
    file_path: &Path,
    already_on_disk: u64,
    body: InboundBody,
) -> Result<u64, AppendError> {
    let limit = max_docker_blob_bytes();
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(file_path)
        .await
        .map_err(|_| AppendError::Write)?;

    let mut written: u64 = 0;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AppendError::Read)?;

        written = written.saturating_add(chunk.len() as u64);
        if already_on_disk.saturating_add(written) > limit {
            return Err(AppendError::TooLarge(limit));
        }

        file.write_all(&chunk)
            .await
            .map_err(|_| AppendError::Write)?;
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

pub fn register_routes() {
    registry::register_routes();
    blob::register_routes();
    manifest::register_routes();
    token::register_routes();
}
