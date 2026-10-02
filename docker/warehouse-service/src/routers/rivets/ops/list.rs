//! Per-package version listing and the catalog (latest offerable version per package).

use crate::domain::rivet::{RivetPackage, latest_of, sort_newest_first};
use crate::routers::rivets::ops::{RivetView, disabled, error, find_name, json_ok};
use crate::routers::rivets::valid_name;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Path, Response, get, http::StatusCode};
use std::collections::BTreeMap;

#[get("/api/v1/rivets/{name}")]
#[tracing::instrument]
pub async fn versions(Inject(db): Inject<Db>, Path(name): Path<String>) -> Response {
    if !crate::routers::rivets_enabled() {
        return disabled();
    }
    if !valid_name(&name) {
        return json_ok(&Vec::<RivetView>::new());
    }

    let mut packages = match find_name(&db, &name).await {
        Ok(packages) => packages,
        Err(response) => return response,
    };
    sort_newest_first(&mut packages);

    json_ok(&packages.iter().map(RivetView::from).collect::<Vec<_>>())
}

#[get("/api/v1/rivets")]
#[tracing::instrument]
pub async fn catalog(Inject(db): Inject<Db>) -> Response {
    if !crate::routers::rivets_enabled() {
        return disabled();
    }

    let all = match db.repository::<RivetPackage>().list().await {
        Ok(all) => all,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    json_ok(&latest_per_name(&all))
}

/// One entry per package name: its newest non-yanked version, sorted by name. A package whose
/// every version is yanked is left out.
pub fn latest_per_name(all: &[RivetPackage]) -> Vec<RivetView> {
    let mut by_name: BTreeMap<&str, Vec<RivetPackage>> = BTreeMap::new();
    for package in all {
        by_name
            .entry(package.name.as_str())
            .or_default()
            .push(package.clone());
    }
    by_name
        .values()
        .filter_map(|rows| latest_of(rows))
        .map(RivetView::from)
        .collect()
}

pub fn register_routes() {
    let _ = versions as fn(_, _) -> _;
    let _ = catalog as fn(_) -> _;
}
