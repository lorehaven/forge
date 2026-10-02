//! The handlers, and what they all share.

use crate::domain::rivet::RivetPackage;
use chrono::{DateTime, Utc};
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Response, http::StatusCode};
use serde::Serialize;

pub use crate::routers::artifacts::ops::{Actor, error, json_ok, not_found};

pub mod download;
pub mod list;
pub mod metadata;
pub mod publish;
pub mod unyank;
pub mod yank;

/// The wire shape every read endpoint returns. `id` is omitted (internal storage key only).
#[derive(Serialize)]
pub struct RivetView {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub namespace: Option<String>,
    pub filename: String,
    pub size_bytes: i64,
    pub sha256: String,
    /// The archive's own `rivet.toml`, parsed.
    pub manifest: serde_json::Value,
    pub uploaded_by: String,
    pub yanked: bool,
    pub created_at: DateTime<Utc>,
}

impl From<&RivetPackage> for RivetView {
    fn from(package: &RivetPackage) -> Self {
        Self {
            name: package.name.clone(),
            version: package.version.clone(),
            description: package.description.clone(),
            namespace: package.namespace.clone(),
            filename: package.filename.clone(),
            size_bytes: package.size_bytes,
            sha256: package.sha256.clone(),
            manifest: package.manifest.0.clone(),
            uploaded_by: package.uploaded_by.clone(),
            yanked: package.yanked,
            created_at: package.created_at,
        }
    }
}

/// Same 404 as "no such version" - whether rivets are enabled isn't for an unauthorized caller to learn.
pub fn disabled() -> Response {
    not_found("rivet storage is not enabled")
}

/// Every row for one package name.
pub async fn find_name(db: &Db, name: &str) -> Result<Vec<RivetPackage>, Response> {
    db.repository::<RivetPackage>()
        .find_by("name", name)
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))
}

/// Resolves a `{version}` path segment, where `latest` means the newest non-yanked version.
/// `Ok(None)` is "no such package or version".
pub async fn resolve(db: &Db, name: &str, version: &str) -> Result<Option<RivetPackage>, Response> {
    if !crate::routers::rivets::valid_name(name) {
        return Ok(None);
    }

    if version == "latest" {
        let versions = find_name(db, name).await?;
        return Ok(crate::domain::rivet::latest_of(&versions).cloned());
    }
    if !crate::routers::rivets::valid_version(version) {
        return Ok(None);
    }

    db.repository::<RivetPackage>()
        .read(&RivetPackage::id_for(name, version))
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))
}

pub fn register_routes() {
    download::register_routes();
    list::register_routes();
    metadata::register_routes();
    publish::register_routes();
    unyank::register_routes();
    yank::register_routes();
}
