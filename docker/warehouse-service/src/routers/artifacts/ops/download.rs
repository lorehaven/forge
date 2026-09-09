//! `GET /api/v1/artifacts/{program}/{platform}/{version_code}/download` -
//! fetch the bytes.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::artifact_file_path;
use crate::routers::artifacts::ops::{disabled, error, not_found};
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder, get, web};
use quench_db::prelude::{Crud, Db};

#[get("/{program}/{platform}/{version_code}/download")]
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

    let version = match db.repository::<ArtifactVersion>().read(&id).await {
        Ok(Some(version)) => version,
        _ => return not_found("program or version not found"),
    };

    serve(&version).await
}

/// Shared by `download::handle`, `latest::download` and the `/api/v1/apk`
/// alias, once each has resolved which [`ArtifactVersion`] it means.
pub async fn serve(version: &ArtifactVersion) -> HttpResponse {
    let Some(platform) = Platform::parse(&version.platform) else {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "catalog row has an unknown platform",
        );
    };
    let Some(path) = artifact_file_path(
        &version.program,
        platform,
        version.version_code,
        &version.filename,
    ) else {
        return not_found("program or version not found");
    };

    let data = match tokio::fs::read(&path).await {
        Ok(data) => data,
        Err(_) => return not_found("program or version not found"),
    };

    let content_type = if version.format == "apk" {
        "application/vnd.android.package-archive"
    } else {
        "application/octet-stream"
    };

    HttpResponse::Ok()
        .content_type(content_type)
        .append_header(("Content-Length", data.len()))
        .append_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", version.filename),
        ))
        .body(data)
}
