//! `GET /api/v1/rivets/{name}/{version}/download` - the archive; `latest` is accepted.
//! A yanked version still downloads by exact version, so an install already in flight finishes.

use crate::domain::rivet::RivetPackage;
use crate::routers::rivets::ops::{disabled, not_found, resolve};
use crate::routers::rivets::package_file_path;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Response, get, http::StatusCode};

#[get("/api/v1/rivets/{name}/{version}/download")]
#[tracing::instrument]
pub async fn handle(
    Inject(db): Inject<Db>,
    Path((name, version)): Path<(String, String)>,
) -> Response {
    if !crate::routers::rivets_enabled() {
        return disabled();
    }

    match resolve(&db, &name, &version).await {
        Ok(Some(package)) => serve(&package).await,
        Ok(None) => not_found("package or version not found"),
        Err(response) => response,
    }
}

async fn serve(package: &RivetPackage) -> Response {
    let Some(path) = package_file_path(&package.name, &package.version) else {
        return not_found("package or version not found");
    };
    let Ok(data) = tokio::fs::read(&path).await else {
        return not_found("package or version not found");
    };

    // `from_bytes` sets content-length itself. The digest lets a client verify without a second call.
    Response::from_bytes(StatusCode::OK, data.into())
        .header("content-type", "application/zstd")
        .header("x-rivet-sha256", package.sha256.clone())
        .header(
            "content-disposition",
            format!("attachment; filename=\"{}\"", package.filename),
        )
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
