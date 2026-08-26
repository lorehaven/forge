//! `DELETE /api/v1/apk/{package}/{version_code}/yank` - hide a version from
//! `latest` and the catalog without deleting it.
//!
//! Mirrors `crates::ops::yank`: an app store shouldn't offer a yanked build
//! to a new install, but a device that already has it installed - or one
//! mid-download - should still be able to fetch it by exact version. Content
//! and history stay; only visibility changes.

use crate::domain::apk::ApkVersion;
use crate::routers::apk::ops::{disabled, error, not_found};
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder, delete, web};
use quench_db::prelude::{Crud, Db};
use serde::Serialize;

#[derive(Serialize)]
pub struct OkResponse {
    ok: bool,
}

#[delete("/{package}/{version_code}/yank")]
#[tracing::instrument]
pub async fn handle(db: web::Data<Db>, path: web::Path<(String, i64)>) -> impl Responder {
    if !crate::routers::apk_enabled() {
        return disabled();
    }

    set_yanked(&db, path.into_inner(), true).await
}

/// Shared by `yank::handle` and `unyank::handle`.
pub async fn set_yanked(
    db: &Db,
    (package_name, version_code): (String, i64),
    value: bool,
) -> HttpResponse {
    let id = ApkVersion::id_for(&package_name, version_code);
    let repo = db.repository::<ApkVersion>();

    let Some(mut version) = (match repo.read(&id).await {
        Ok(version) => version,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }) else {
        return not_found("package or version not found");
    };

    version.yanked = value;

    match repo.update(&version).await {
        Ok(_) => HttpResponse::Ok().json(OkResponse { ok: true }),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}
