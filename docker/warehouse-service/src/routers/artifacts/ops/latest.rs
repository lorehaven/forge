//! `GET /api/v1/artifacts/{program}/{platform}/latest` and
//! `.../latest/download` - the highest `version_code` that isn't yanked.
//!
//! Registered before [`super::metadata::handle`] and [`super::download`] in
//! [`crate::routers::artifacts::scope`] - see that function's doc comment for
//! why the order matters.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::{
    ArtifactView, disabled, find_program_platform, latest_of, not_found,
};
use actix_web::{HttpResponse, Responder, get, web};
use quench_db::prelude::Db;

#[get("/{program}/{platform}/latest")]
#[tracing::instrument]
pub async fn metadata(db: web::Data<Db>, path: web::Path<(String, String)>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    let (program, platform_raw) = path.into_inner();
    match resolve_latest(&db, &program, &platform_raw).await {
        Ok(Some(version)) => HttpResponse::Ok().json(ArtifactView::from(&version)),
        Ok(None) => not_found("program has no offerable version for this platform"),
        Err(response) => response,
    }
}

#[get("/{program}/{platform}/latest/download")]
#[tracing::instrument]
pub async fn download(db: web::Data<Db>, path: web::Path<(String, String)>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    let (program, platform_raw) = path.into_inner();
    match resolve_latest(&db, &program, &platform_raw).await {
        Ok(Some(version)) => super::download::serve(&version).await,
        Ok(None) => not_found("program has no offerable version for this platform"),
        Err(response) => response,
    }
}

/// Shared by both handlers and the `/api/v1/apk` alias.
pub async fn resolve_latest(
    db: &Db,
    program: &str,
    platform_raw: &str,
) -> Result<Option<ArtifactVersion>, HttpResponse> {
    let Some(platform) = Platform::parse(platform_raw) else {
        return Ok(None);
    };
    let versions = find_program_platform(db, program, platform).await?;
    Ok(latest_of(&versions).cloned())
}
