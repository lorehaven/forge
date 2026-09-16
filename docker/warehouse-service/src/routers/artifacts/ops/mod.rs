//! The handlers, and what they all share.

use crate::domain::artifact::{ArtifactVersion, Platform};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::http::routers::ui::get_user_from_req;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{FromRequest, HttpError, Request, Response, http::StatusCode};
use serde::Serialize;
use serde_json::json;

pub mod download;
pub mod latest;
pub mod list;
pub mod metadata;
pub mod publish;
pub mod unyank;
pub mod yank;

/// The wire shape every read endpoint returns. `id` is omitted (internal storage key only);
/// `min_sdk_version`/`target_sdk_version`/`permissions` are Android-only, empty elsewhere.
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

/// The highest non-yanked `version_code`, or `None` if every version is yanked.
pub fn latest_of(versions: &[ArtifactVersion]) -> Option<&ArtifactVersion> {
    versions
        .iter()
        .filter(|version| !version.yanked)
        .max_by_key(|version| version.version_code)
}

/// Every row for one program+platform; `Crud::find_by` is equality-only, so platform is post-filtered.
pub async fn find_program_platform(
    db: &Db,
    program: &str,
    platform: Platform,
) -> Result<Vec<ArtifactVersion>, Response> {
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

pub fn json_ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn error(status: StatusCode, message: &str) -> Response {
    Response::json(status, &json!({ "error": message }))
        .unwrap_or_else(|_| Response::text(status, message))
}

pub fn not_found(message: &str) -> Response {
    error(StatusCode::NOT_FOUND, message)
}

/// Same 404 as "no such version" - whether artifacts are enabled isn't for an unauthorized caller to learn.
pub fn disabled() -> Response {
    not_found("artifact storage is not enabled")
}

/// Who is making this request, for `uploaded_by`. Falls back to "dev" when auth is disabled.
pub struct Actor(pub String);

#[async_trait]
impl FromRequest for Actor {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        if let Some(claims) = req.extensions().get::<Claims>() {
            return Ok(Self(claims.sub.clone()));
        }
        // Auth disabled or extensions not populated yet - same fallback as other actor helpers.
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self("dev".to_string()));
        };
        let username = get_user_from_req(req, &config)
            .await
            .map(|claims| claims.sub)
            .unwrap_or_else(|| "dev".to_string());
        Ok(Self(username))
    }
}

pub fn register_routes() {
    download::register_routes();
    list::register_routes();
    metadata::register_routes();
    publish::register_routes();
    unyank::register_routes();
    yank::register_routes();
}
