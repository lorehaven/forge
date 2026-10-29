//! `DELETE .../{version_code}/yank` - hides a version from `latest`/catalog without deleting it,
//! so an exact-version fetch (a device mid-download) still works. Mirrors `crates::ops::yank`.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::{FormatQuery, disabled, error, find_versions, not_found};
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Path, Query, Response, delete, http::StatusCode};
use quench_serde::Serialize;

#[derive(Debug, Serialize)]
pub struct OkResponse {
    ok: bool,
}

#[delete("/api/v1/artifacts/{program}/{platform}/{version_code}/yank")]
#[quench_log::instrument]
pub async fn handle(
    Inject(db): Inject<Db>,
    Path((program, platform_raw, version_code)): Path<(String, String, i64)>,
    Query(query): Query<FormatQuery>,
) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    set_yanked(
        &db,
        &program,
        &platform_raw,
        version_code,
        query.wanted().as_deref(),
        true,
    )
    .await
}

/// Shared by `yank::handle`, `unyank::handle`, the `/api/v1/apk` alias and the catalog page: every format of the
/// version, or only `format`. A release is withdrawn as a whole unless a format is named.
pub async fn set_yanked(
    db: &Db,
    program: &str,
    platform_raw: &str,
    version_code: i64,
    format: Option<&str>,
    value: bool,
) -> Response {
    let Some(platform) = Platform::parse(platform_raw) else {
        return not_found("program or version not found");
    };
    let repo = db.repository::<ArtifactVersion>();

    let found = match find_versions(db, program, platform, version_code, format).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    if found.is_empty() {
        return not_found("program or version not found");
    }

    for mut version in found {
        version.yanked = value;
        if let Err(e) = repo.update(&version).await {
            return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
        }
    }
    Response::json(StatusCode::OK, &OkResponse { ok: true })
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = handle as fn(_, _, _) -> _;
}
