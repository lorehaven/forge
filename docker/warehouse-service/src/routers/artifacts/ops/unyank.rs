//! `PUT /api/v1/artifacts/{program}/{platform}/{version_code}/unyank` - undo a
//! yank.

use crate::routers::artifacts::ops::disabled;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Response, put};

#[put("/api/v1/artifacts/{program}/{platform}/{version_code}/unyank")]
#[tracing::instrument]
pub async fn handle(
    Inject(db): Inject<Db>,
    Path((program, platform_raw, version_code)): Path<(String, String, i64)>,
) -> Response {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    super::yank::set_yanked(&db, &program, &platform_raw, version_code, false).await
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
