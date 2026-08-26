//! `GET /api/v1/apk/{package}/latest` and `.../latest/download` - the
//! highest `version_code` that isn't yanked.
//!
//! Registered before [`super::metadata::handle`] and [`super::download`] in
//! [`crate::routers::apk::scope`] - see that function's doc comment for why
//! the order matters.

use crate::domain::apk::ApkVersion;
use crate::routers::apk::ops::{VersionView, disabled, error, latest_of, not_found};
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder, get, web};
use quench_db::prelude::{Crud, Db};

#[get("/{package}/latest")]
#[tracing::instrument]
pub async fn metadata(db: web::Data<Db>, path: web::Path<String>) -> impl Responder {
    if !crate::routers::apk_enabled() {
        return disabled();
    }

    match resolve_latest(&db, &path.into_inner()).await {
        Ok(Some(version)) => HttpResponse::Ok().json(VersionView::from(&version)),
        Ok(None) => not_found("package has no offerable version"),
        Err(response) => response,
    }
}

#[get("/{package}/latest/download")]
#[tracing::instrument]
pub async fn download(db: web::Data<Db>, path: web::Path<String>) -> impl Responder {
    if !crate::routers::apk_enabled() {
        return disabled();
    }

    match resolve_latest(&db, &path.into_inner()).await {
        Ok(Some(version)) => super::download::serve(&version).await,
        Ok(None) => not_found("package has no offerable version"),
        Err(response) => response,
    }
}

async fn resolve_latest(db: &Db, package_name: &str) -> Result<Option<ApkVersion>, HttpResponse> {
    let versions = db
        .repository::<ApkVersion>()
        .find_by("package_name", package_name)
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok(latest_of(&versions).cloned())
}
