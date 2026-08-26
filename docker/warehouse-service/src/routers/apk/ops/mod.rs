//! The handlers, and what they all share.

use crate::domain::apk::ApkVersion;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, http::StatusCode};
use chrono::{DateTime, Utc};
use quench_auth::prelude::Claims;
use serde::Serialize;
use serde_json::json;

pub mod download;
pub mod latest;
pub mod list;
pub mod metadata;
pub mod publish;
pub mod unyank;
pub mod yank;

/// What a version's catalog row looks like over the wire - every read
/// endpoint (`metadata`, `list`, `latest`) returns this shape, so a caller
/// only has to parse one schema regardless of which of them it hit.
/// `id` is deliberately not exposed: it is this module's own
/// `<package>@<version_code>` storage key, not something a caller
/// constructs or needs.
#[derive(Serialize)]
pub struct VersionView {
    pub package_name: String,
    pub version_code: i64,
    pub version_name: String,
    pub min_sdk_version: Option<i32>,
    pub target_sdk_version: Option<i32>,
    pub label: Option<String>,
    pub permissions: Vec<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub uploaded_by: String,
    pub yanked: bool,
    pub created_at: DateTime<Utc>,
}

impl From<&ApkVersion> for VersionView {
    fn from(version: &ApkVersion) -> Self {
        Self {
            package_name: version.package_name.clone(),
            version_code: version.version_code,
            version_name: version.version_name.clone(),
            min_sdk_version: version.min_sdk_version,
            target_sdk_version: version.target_sdk_version,
            label: version.label.clone(),
            permissions: version.permissions.0.clone(),
            size_bytes: version.size_bytes,
            sha256: version.sha256.clone(),
            uploaded_by: version.uploaded_by.clone(),
            yanked: version.yanked,
            created_at: version.created_at,
        }
    }
}

/// The highest `version_code` in `versions` that isn't yanked - what
/// `latest` and the catalog listing both resolve to. `None` when every
/// version of the package has been yanked.
pub fn latest_of(versions: &[ApkVersion]) -> Option<&ApkVersion> {
    versions
        .iter()
        .filter(|version| !version.yanked)
        .max_by_key(|version| version.version_code)
}

pub fn error(status: StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(json!({ "error": message }))
}

pub fn not_found(message: &str) -> HttpResponse {
    error(StatusCode::NOT_FOUND, message)
}

/// "APK storage is not enabled" and "no such package/version" both answer
/// the same 404, deliberately: whether this deployment *could* serve APKs is
/// not something an unauthorised caller learns by asking.
pub fn disabled() -> HttpResponse {
    not_found("apk storage is not enabled")
}

/// Who is making this request, for the catalog's `uploaded_by` column.
///
/// `Auth` (mounted around the whole scope) has already put [`Claims`] in the
/// request's extensions by the time a handler runs - see
/// `routers::files::authz::can_on_storage` for the same read. With auth
/// disabled there is nothing there at all, so this falls back to a fixed
/// name the same way `workbench`/`conveyor`'s own `actor()` helpers do for
/// their `author`/`reporter` columns.
pub fn actor(request: &HttpRequest) -> String {
    request
        .extensions()
        .get::<Claims>()
        .map(|claims| claims.sub.clone())
        .unwrap_or_else(|| "dev".to_string())
}
