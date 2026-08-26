//! `PUT /api/v1/apk/{package}/{version_code}/unyank` - undo a yank.

use crate::routers::apk::ops::disabled;
use actix_web::{Responder, put, web};
use quench_db::prelude::Db;

#[put("/{package}/{version_code}/unyank")]
#[tracing::instrument]
pub async fn handle(db: web::Data<Db>, path: web::Path<(String, i64)>) -> impl Responder {
    if !crate::routers::apk_enabled() {
        return disabled();
    }

    super::yank::set_yanked(&db, path.into_inner(), false).await
}
