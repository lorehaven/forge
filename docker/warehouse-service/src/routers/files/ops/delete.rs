//! `DELETE /api/v1/files/{storage}/file?path=…` - remove a file.
//!
//! Only ever one file. A recursive delete addressed by path is one typo away
//! from emptying a storage, and nothing in the estate needs it: conveyor's
//! artifacts are cleaned up per run, by name.

use super::{
    ResolvedStorage, authorize, dynamic_path, error, forbidden, not_found, resolve_storage,
};
use crate::domain::storage_file;
use crate::routers::files::{FileQuery, dynamic};
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, delete, web};
use quench_db::prelude::Db;

#[delete("/{storage}/file")]
#[tracing::instrument(skip(request))]
pub async fn handle(
    request: HttpRequest,
    db: web::Data<Db>,
    storage: web::Path<String>,
    query: web::Query<FileQuery>,
) -> impl Responder {
    let storage_name = storage.into_inner();
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return *response,
    };

    if !authorize(&request, &resolved, "write") {
        return forbidden("write access to this storage is required");
    }

    match resolved {
        ResolvedStorage::Static(storage) => {
            let target = match super::static_target_or_error(storage, &query.path).await {
                Ok(target) => target,
                Err(response) => return *response,
            };

            if !super::download::is_file(&target).await {
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
        ResolvedStorage::Dynamic(storage) => {
            let path = match dynamic_path(&query.path) {
                Ok(path) => path,
                Err(response) => return *response,
            };
            let Some(root) = dynamic::root() else {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "this deployment has no DYNAMIC_STORAGE_ROOT configured",
                );
            };

            let deleted = storage_file::delete_file(&db, &storage.name, &path, |sha256| {
                dynamic::blob_path(&root, sha256)
            })
            .await;

            match deleted {
                Ok(true) => {
                    tracing::info!("deleted `{path}` from dynamic storage `{}`", storage.name);
                    HttpResponse::NoContent().finish()
                }
                Ok(false) => not_found("no such file"),
                Err(problem) => {
                    tracing::error!("dynamic delete failed: {problem}");
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "could not delete the file",
                    )
                }
            }
        }
    }
}
