//! The crates registry: its crates, their versions from the index, and yanking.

use super::{Session, error, internal, no_content, ok};
use crate::domain::crate_index::{IndexRecord, list_crates, list_versions};
use crate::routers::crates::ops::yank::set_yanked;
use quench_http::prelude::{Json, Path, Response, get, http::StatusCode, post};
use warehouse_core::{CrateDep, CrateRef, CrateVersion};

#[must_use]
pub fn version_of(record: IndexRecord) -> CrateVersion {
    let mut features: Vec<String> = record.features.keys().cloned().collect();
    for name in record.features2.iter().flat_map(|f| f.keys()) {
        if !features.contains(name) {
            features.push(name.clone());
        }
    }
    features.sort_unstable();
    CrateVersion {
        vers: record.vers,
        yanked: record.yanked,
        cksum: record.cksum,
        rust_version: record.rust_version,
        links: record.links,
        features,
        deps: record
            .deps
            .into_iter()
            .map(|dep| CrateDep {
                name: dep.name,
                req: dep.req,
                optional: dep.optional,
                kind: dep.kind,
                target: dep.target,
                package: dep.package,
            })
            .collect(),
    }
}

#[get("/ui/api/crates")]
pub async fn crates(session: Session) -> Response {
    if let Err(response) = session.require() {
        return response;
    }
    ok(&list_crates())
}

/// A crate's versions, oldest first (the order of its index file). 404 for a crate that is not in the index.
#[get("/ui/api/crates/{name}/versions")]
pub async fn versions(session: Session, Path(name): Path<String>) -> Response {
    if let Err(response) = session.require() {
        return response;
    }
    if !list_crates().contains(&name) {
        return error(StatusCode::NOT_FOUND, "api_error_crate_version_not_found");
    }
    ok(&list_versions(&name)
        .into_iter()
        .map(version_of)
        .collect::<Vec<_>>())
}

async fn yank_state(session: Session, body: CrateRef, yanked: bool) -> Response {
    if let Err(response) = session.require() {
        return response;
    }
    match set_yanked(&body.name, &body.version, yanked).await {
        Ok(true) => no_content(),
        Ok(false) => error(StatusCode::NOT_FOUND, "api_error_crate_version_not_found"),
        Err(msg) => {
            quench_log::error!("Failed to change yank state: {}", msg);
            internal()
        }
    }
}

#[post("/ui/api/crates/yank")]
pub async fn yank(session: Session, Json(body): Json<CrateRef>) -> Response {
    yank_state(session, body, true).await
}

#[post("/ui/api/crates/unyank")]
pub async fn unyank(session: Session, Json(body): Json<CrateRef>) -> Response {
    yank_state(session, body, false).await
}

pub fn register_routes() {
    let _ = crates as fn(_) -> _;
    let _ = versions as fn(_, _) -> _;
    let _ = yank as fn(_, _) -> _;
    let _ = unyank as fn(_, _) -> _;
}
