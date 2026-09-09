//! The handlers, and what they all share.

use crate::domain::artifact::{ArtifactVersion, Platform};
use actix_web::{HttpMessage, HttpRequest, HttpResponse, http::StatusCode};
use chrono::{DateTime, Utc};
use quench_auth::prelude::Claims;
use quench_db::prelude::{Crud, Db};
use serde::Serialize;
use serde_json::json;

pub mod download;
pub mod latest;
pub mod list;
pub mod metadata;
pub mod publish;
pub mod unyank;
pub mod yank;

/// What an artifact's catalog row looks like over the wire - every read
/// endpoint (`metadata`, `list`, `latest`) returns this shape, so a caller
/// only has to parse one schema regardless of which of them it hit.
/// `id` is deliberately not exposed: it is this module's own
/// `<program>/<platform>@<version_code>` storage key, not something a caller
/// constructs or needs. `min_sdk_version` / `target_sdk_version` /
/// `permissions` are Android-only and come out empty for other platforms.
#[derive(Serialize)]
pub struct ArtifactView {
    pub program: String,
    pub platform: String,
    pub arch: Option<String>,
    pub format: String,
    pub version_code: i64,
    pub version_name: String,
    pub filename: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub label: Option<String>,
    pub min_sdk_version: Option<i32>,
    pub target_sdk_version: Option<i32>,
    pub permissions: Vec<String>,
    pub uploaded_by: String,
    pub yanked: bool,
    pub created_at: DateTime<Utc>,
}

impl From<&ArtifactVersion> for ArtifactView {
    fn from(version: &ArtifactVersion) -> Self {
        Self {
            program: version.program.clone(),
            platform: version.platform.clone(),
            arch: version.arch.clone(),
            format: version.format.clone(),
            version_code: version.version_code,
            version_name: version.version_name.clone(),
            filename: version.filename.clone(),
            size_bytes: version.size_bytes,
            sha256: version.sha256.clone(),
            label: version.label.clone(),
            min_sdk_version: version.metadata.0.min_sdk_version,
            target_sdk_version: version.metadata.0.target_sdk_version,
            permissions: version.metadata.0.permissions.clone(),
            uploaded_by: version.uploaded_by.clone(),
            yanked: version.yanked,
            created_at: version.created_at,
        }
    }
}

/// The highest `version_code` in `versions` that isn't yanked - what
/// `latest` and the catalog listing both resolve to. `None` when every
/// version has been yanked.
pub fn latest_of(versions: &[ArtifactVersion]) -> Option<&ArtifactVersion> {
    versions
        .iter()
        .filter(|version| !version.yanked)
        .max_by_key(|version| version.version_code)
}

/// Every row for one program on one platform. `Crud::find_by` only does
/// single-column equality, so the platform is filtered here - the same
/// post-filter `list::catalog` already does after a `list()`.
pub async fn find_program_platform(
    db: &Db,
    program: &str,
    platform: Platform,
) -> Result<Vec<ArtifactVersion>, HttpResponse> {
    let rows = db
        .repository::<ArtifactVersion>()
        .find_by("program", program)
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    Ok(rows
        .into_iter()
        .filter(|row| row.platform == platform.as_str())
        .collect())
}

pub fn error(status: StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(json!({ "error": message }))
}

pub fn not_found(message: &str) -> HttpResponse {
    error(StatusCode::NOT_FOUND, message)
}

/// "Artifact storage is not enabled" and "no such program/version" both answer
/// the same 404, deliberately: whether this deployment *could* serve artifacts
/// is not something an unauthorised caller learns by asking.
pub fn disabled() -> HttpResponse {
    not_found("artifact storage is not enabled")
}

/// Who is making this request, for the catalog's `uploaded_by` column.
///
/// `Auth` (mounted around the whole scope) has already put [`Claims`] in the
/// request's extensions by the time a handler runs. With auth disabled there
/// is nothing there at all, so this falls back to a fixed name the same way
/// `workbench`/`conveyor`'s own `actor()` helpers do.
pub fn actor(request: &HttpRequest) -> String {
    request
        .extensions()
        .get::<Claims>()
        .map(|claims| claims.sub.clone())
        .unwrap_or_else(|| "dev".to_string())
}
