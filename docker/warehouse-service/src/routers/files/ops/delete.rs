//! `DELETE /api/v1/files/{storage}/file?path=…` - remove a file.
//!
//! Only ever one file. A recursive delete addressed by path is one typo away
//! from emptying a storage, and nothing in the estate needs it: conveyor's
//! artifacts are cleaned up per run, by name.

use super::{download::is_file, error, not_found, target_or_error};
use crate::routers::files::FileQuery;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, Responder, delete, web};

#[delete("/{storage}/file")]
#[tracing::instrument]
pub async fn handle(storage: web::Path<String>, query: web::Query<FileQuery>) -> impl Responder {
    let storage_name = storage.into_inner();
    let (storage, target) = match target_or_error(&storage_name, &query.path).await {
        Ok(resolved) => resolved,
        Err(response) => return *response,
    };

    if !is_file(&target).await {
        // Covers both "not there" and "there but a directory". Neither is
        // something this endpoint will remove, and distinguishing them tells a
        // caller what the directory tree looks like.
        return not_found("no such file");
    }

    if tokio::fs::remove_file(&target).await.is_err() {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not delete the file",
        );
    }

    tracing::info!("deleted `{}` from storage `{}`", query.path, storage.name);

    HttpResponse::NoContent().finish()
}
