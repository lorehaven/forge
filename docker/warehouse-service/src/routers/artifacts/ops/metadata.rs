//! `GET /api/v1/artifacts/{program}/{platform}/{version_code}` - one
//! artifact's catalog row.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::ops::{ArtifactView, disabled, not_found};
use actix_web::{HttpResponse, Responder, get, web};
use quench_db::prelude::{Crud, Db};

#[get("/{program}/{platform}/{version_code}")]
#[tracing::instrument]
pub async fn handle(db: web::Data<Db>, path: web::Path<(String, String, i64)>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    let (program, platform_raw, version_code) = path.into_inner();
    let Some(platform) = Platform::parse(&platform_raw) else {
        return not_found("program or version not found");
    };
    let id = ArtifactVersion::id_for(&program, platform, version_code);

    match db.repository::<ArtifactVersion>().read(&id).await {
        Ok(Some(version)) => HttpResponse::Ok().json(ArtifactView::from(&version)),
        _ => not_found("program or version not found"),
    }
}
