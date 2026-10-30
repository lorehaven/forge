//! The rivet package registry: its published versions, and yanking them.

use super::{Session, error, ok};
use crate::domain::rivet::{RivetPackage, sort_newest_first};
use crate::routers::rivets::ops::yank::set_yanked;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Json, Response, get, http::StatusCode, post};
use warehouse_core::{MetaPair, RivetEntry, RivetRef};

#[must_use]
pub fn entry_of(package: &RivetPackage) -> RivetEntry {
    let requires = &package.manifest["requires"];
    RivetEntry {
        name: package.name.clone(),
        version: package.version.clone(),
        size_bytes: package.size_bytes,
        sha256: package.sha256.clone(),
        uploaded_by: package.uploaded_by.clone(),
        yanked: package.yanked,
        published: package.created_at.format("%Y-%m-%d %H:%M UTC"),
        description: package.description.clone(),
        namespace: package.namespace.clone(),
        requires_riveter: requires["riveter"].as_str().map(str::to_string),
        requires_packages: requires["packages"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        meta: package.manifest["meta"]
            .as_object()
            .map(|meta| {
                meta.iter()
                    .map(|(key, value)| MetaPair {
                        key: key.clone(),
                        value: value
                            .as_str()
                            .map_or_else(|| value.to_string(), str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Every version of every package, each package newest first. A disabled registry or an unreachable database is an empty
/// list, as for the old page.
#[get("/ui/api/rivets")]
pub async fn list(session: Session, Inject(db): Inject<Db>) -> Response {
    if let Err(response) = session.require() {
        return response;
    }
    let mut packages = if crate::routers::rivets_enabled() {
        db.repository::<RivetPackage>()
            .list()
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    sort_newest_first(&mut packages);
    ok(&packages.iter().map(entry_of).collect::<Vec<_>>())
}

async fn yank_state(session: Session, db: &Db, body: RivetRef, yanked: bool) -> Response {
    if let Err(response) = session.manager() {
        return response;
    }
    if !crate::routers::rivets_enabled() {
        return error(StatusCode::NOT_FOUND, "api_error_rivets_disabled");
    }
    set_yanked(db, &body.name, &body.version, yanked).await
}

#[post("/ui/api/rivets/yank")]
pub async fn yank(
    session: Session,
    Inject(db): Inject<Db>,
    Json(body): Json<RivetRef>,
) -> Response {
    yank_state(session, &db, body, true).await
}

#[post("/ui/api/rivets/unyank")]
pub async fn unyank(
    session: Session,
    Inject(db): Inject<Db>,
    Json(body): Json<RivetRef>,
) -> Response {
    yank_state(session, &db, body, false).await
}

pub fn register_routes() {
    let _ = list as fn(_, _) -> _;
    let _ = yank as fn(_, _, _) -> _;
    let _ = unyank as fn(_, _, _) -> _;
}
