//! `GET .../{version_code}/download` and `.../latest/download` in one handler - see `super::latest`.

use crate::domain::artifact::{ArtifactVersion, Platform};
use crate::routers::artifacts::artifact_file_path;
use crate::routers::artifacts::ops::{disabled, error, latest, not_found};
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Path, Response, get, http::StatusCode};

#[get("/api/v1/artifacts/{program}/{platform}/{version_code}/download")]
#[tracing::instrument]
pub async fn handle(
    Inject(db): Inject<Db>,
    Path((program, platform_raw, version_code_raw)): Path<(String, String, String)>,
) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    if version_code_raw == "latest" {
        return match latest::resolve_latest(&db, &program, &platform_raw).await {
            Ok(Some(version)) => serve(&version).await,
            Ok(None) => not_found("program has no offerable version for this platform"),
            Err(response) => response,
        };
    }

    let Ok(version_code) = version_code_raw.parse::<i64>() else {
        return not_found("program or version not found");
    };
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

/// Shared by `handle` and the `/api/v1/apk` alias once each resolves the version.
pub async fn serve(version: &ArtifactVersion) -> Response {
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

    // `from_bytes` sets content-length itself.
    Response::from_bytes(StatusCode::OK, data.into())
        .header("content-type", content_type)
        .header(
            "content-disposition",
            format!("attachment; filename=\"{}\"", version.filename),
        )
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
