//! `DELETE .../{version}/yank` - hides a version from `latest`/catalog without deleting it,
//! so an exact-version fetch (an install mid-download) still works.

use crate::domain::rivet::RivetPackage;
use crate::routers::rivets::ops::{disabled, error, not_found};
use crate::routers::rivets::{valid_name, valid_version};
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Path, Response, delete, http::StatusCode};
use serde::Serialize;

#[derive(Serialize)]
pub struct OkResponse {
    ok: bool,
}

#[delete("/api/v1/rivets/{name}/{version}/yank")]
#[tracing::instrument]
pub async fn handle(
    Inject(db): Inject<Db>,
    Path((name, version)): Path<(String, String)>,
) -> Response {
    if !crate::routers::rivets_enabled() {
        return disabled();
    }

    set_yanked(&db, &name, &version, true).await
}

/// Shared by `yank::handle` and `unyank::handle`.
pub async fn set_yanked(db: &Db, name: &str, version: &str, value: bool) -> Response {
    if !valid_name(name) || !valid_version(version) {
        return not_found("package or version not found");
    }
    let repo = db.repository::<RivetPackage>();

    let Some(mut package) = (match repo.read(&RivetPackage::id_for(name, version)).await {
        Ok(package) => package,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }) else {
        return not_found("package or version not found");
    };

    package.yanked = value;

    match repo.update(&package).await {
        Ok(_) => Response::json(StatusCode::OK, &OkResponse { ok: true })
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
