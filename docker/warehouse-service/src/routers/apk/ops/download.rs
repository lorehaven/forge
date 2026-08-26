//! `GET /api/v1/apk/{package}/{version_code}/download` - fetch the bytes.

use crate::domain::apk::ApkVersion;
use crate::routers::apk::apk_file_path;
use crate::routers::apk::ops::{disabled, not_found};
use actix_web::{HttpResponse, Responder, get, web};
use quench_db::prelude::{Crud, Db};

#[get("/{package}/{version_code}/download")]
#[tracing::instrument]
pub async fn handle(db: web::Data<Db>, path: web::Path<(String, i64)>) -> impl Responder {
    if !crate::routers::apk_enabled() {
        return disabled();
    }

    let (package_name, version_code) = path.into_inner();
    let id = ApkVersion::id_for(&package_name, version_code);

    let version = match db.repository::<ApkVersion>().read(&id).await {
        Ok(Some(version)) => version,
        _ => return not_found("package or version not found"),
    };

    serve(&version).await
}

/// Shared by `download::handle` and `latest::download`, once each has
/// resolved which [`ApkVersion`] it means.
pub async fn serve(version: &ApkVersion) -> HttpResponse {
    let Some(path) = apk_file_path(&version.package_name, version.version_code) else {
        return not_found("package or version not found");
    };

    let data = match tokio::fs::read(&path).await {
        Ok(data) => data,
        Err(_) => return not_found("package or version not found"),
    };

    HttpResponse::Ok()
        .content_type("application/vnd.android.package-archive")
        .append_header(("Content-Length", data.len()))
        .append_header((
            "Content-Disposition",
            format!(
                "attachment; filename=\"{}-{}.apk\"",
                version.package_name, version.version_code
            ),
        ))
        .body(data)
}
