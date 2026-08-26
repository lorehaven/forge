//! `GET /api/v1/apk/{package}/{version_code}` - one version's catalog row.

use crate::domain::apk::ApkVersion;
use crate::routers::apk::ops::{VersionView, disabled, not_found};
use actix_web::{HttpResponse, Responder, get, web};
use quench_db::prelude::{Crud, Db};

#[get("/{package}/{version_code}")]
#[tracing::instrument]
pub async fn handle(db: web::Data<Db>, path: web::Path<(String, i64)>) -> impl Responder {
    if !crate::routers::apk_enabled() {
        return disabled();
    }

    let (package_name, version_code) = path.into_inner();
    let id = ApkVersion::id_for(&package_name, version_code);

    match db.repository::<ApkVersion>().read(&id).await {
        Ok(Some(version)) => HttpResponse::Ok().json(VersionView::from(&version)),
        _ => not_found("package or version not found"),
    }
}
