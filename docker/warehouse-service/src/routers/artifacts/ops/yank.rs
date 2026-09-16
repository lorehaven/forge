//! `DELETE .../{version_code}/yank` - hides a version from `latest`/catalog without deleting it,
//! so an exact-version fetch (a device mid-download) still works. Mirrors `crates::ops::yank`.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::{disabled, error, not_found};
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Path, Response, delete, http::StatusCode};
use serde::Serialize;

#[derive(Serialize)]
pub struct OkResponse {
    ok: bool,
}

#[delete("/api/v1/artifacts/{program}/{platform}/{version_code}/yank")]
#[tracing::instrument]
pub async fn handle(
    Inject(db): Inject<Db>,
    Path((program, platform_raw, version_code)): Path<(String, String, i64)>,
) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    set_yanked(&db, &program, &platform_raw, version_code, true).await
}

/// Shared by `yank::handle`, `unyank::handle` and the `/api/v1/apk` alias.
pub async fn set_yanked(
    db: &Db,
    program: &str,
    platform_raw: &str,
    version_code: i64,
    value: bool,
) -> Response {
    let Some(platform) = Platform::parse(platform_raw) else {
        return not_found("program or version not found");
    };
    let id = ArtifactVersion::id_for(program, platform, version_code);
    let repo = db.repository::<ArtifactVersion>();

    let Some(mut version) = (match repo.read(&id).await {
        Ok(version) => version,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }) else {
        return not_found("program or version not found");
    };

    version.yanked = value;

    match repo.update(&version).await {
        Ok(_) => Response::json(StatusCode::OK, &OkResponse { ok: true })
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
