//! `DELETE /api/v1/artifacts/{program}/{platform}/{version_code}/yank` - hide
//! a version from `latest` and the catalog without deleting it.
//!
//! Mirrors `crates::ops::yank`: a store shouldn't offer a yanked build to a
//! new install, but a device that already has it - or one mid-download -
//! should still be able to fetch it by exact version. Content and history
//! stay; only visibility changes.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::{disabled, error, not_found};
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder, delete, web};
use quench_db::prelude::{Crud, Db};
use serde::Serialize;

#[derive(Serialize)]
pub struct OkResponse {
    ok: bool,
}

#[delete("/{program}/{platform}/{version_code}/yank")]
#[tracing::instrument]
pub async fn handle(db: web::Data<Db>, path: web::Path<(String, String, i64)>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    let (program, platform_raw, version_code) = path.into_inner();
    set_yanked(&db, &program, &platform_raw, version_code, true).await
}

/// Shared by `yank::handle`, `unyank::handle` and the `/api/v1/apk` alias.
pub async fn set_yanked(
    db: &Db,
    program: &str,
    platform_raw: &str,
    version_code: i64,
    value: bool,
) -> HttpResponse {
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
        Ok(_) => HttpResponse::Ok().json(OkResponse { ok: true }),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}
