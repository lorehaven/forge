//! `PUT /api/v1/artifacts/{program}/{platform}/{version_code}/unyank` - undo a
//! yank.

use crate::routers::artifacts::ops::disabled;
use actix_web::{Responder, put, web};
use quench_db::prelude::Db;

#[put("/{program}/{platform}/{version_code}/unyank")]
#[tracing::instrument]
pub async fn handle(db: web::Data<Db>, path: web::Path<(String, String, i64)>) -> impl Responder {
    if !crate::routers::artifacts_enabled() {
        return disabled();
    }

    let (program, platform_raw, version_code) = path.into_inner();
    super::yank::set_yanked(&db, &program, &platform_raw, version_code, false).await
}
