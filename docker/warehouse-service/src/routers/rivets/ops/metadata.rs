//! `GET /api/v1/rivets/{name}/{version}` - one version's catalog row; `latest` is accepted.

use crate::routers::rivets::ops::{RivetView, disabled, json_ok, not_found, resolve};
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Response, get};

#[get("/api/v1/rivets/{name}/{version}")]
#[tracing::instrument]
pub async fn handle(
    Inject(db): Inject<Db>,
    Path((name, version)): Path<(String, String)>,
) -> Response {
    if !crate::routers::rivets_enabled() {
        return disabled();
    }

    match resolve(&db, &name, &version).await {
        Ok(Some(package)) => json_ok(&RivetView::from(&package)),
        Ok(None) => not_found("package or version not found"),
        Err(response) => response,
    }
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
