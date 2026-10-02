//! `PUT /api/v1/rivets/{name}/{version}/unyank` - undo a yank.

use crate::routers::rivets::ops::disabled;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Response, put};

#[put("/api/v1/rivets/{name}/{version}/unyank")]
#[tracing::instrument]
pub async fn handle(
    Inject(db): Inject<Db>,
    Path((name, version)): Path<(String, String)>,
) -> Response {
    if !crate::routers::rivets_enabled() {
        return disabled();
    }

    super::yank::set_yanked(&db, &name, &version, false).await
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
