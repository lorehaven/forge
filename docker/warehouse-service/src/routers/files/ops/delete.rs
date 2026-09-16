//! `DELETE /api/v1/files/{storage}/file?path=…` - one file only, never recursive.

use super::{
    ResolvedStorage, authorize, dynamic_path, error, forbidden, not_found, resolve_storage,
};
use crate::domain::storage_file;
use crate::routers::files::{FileQuery, OptionalClaims, dynamic};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Query, Response, delete, http::StatusCode};

#[delete("/api/v1/files/{storage}/file")]
#[tracing::instrument(skip(claims, config, db))]
pub async fn handle(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Path(storage_name): Path<String>,
    Query(query): Query<FileQuery>,
) -> Response {
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };

    if !authorize(claims.as_ref(), &config, &resolved, "write") {
        return forbidden("write access to this storage is required");
    }

    match resolved {
        ResolvedStorage::Static(storage) => {
            let target = match super::static_target_or_error(storage, &query.path).await {
                Ok(target) => target,
                Err(response) => return response,
            };

            if !super::download::is_file(&target).await {
                // Covers "not there" and "there but a directory" alike.
                return not_found("no such file");
            }

            if tokio::fs::remove_file(&target).await.is_err() {
                return error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not delete the file",
                );
            }

            tracing::info!("deleted `{}` from storage `{}`", query.path, storage.name);

            Response::new(StatusCode::NO_CONTENT)
        }
        ResolvedStorage::Dynamic(storage) => {
            let path = match dynamic_path(&query.path) {
                Ok(path) => path,
                Err(response) => return response,
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
                    Response::new(StatusCode::NO_CONTENT)
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

pub fn register_routes() {
    let _ = handle as fn(_, _, _, _, _) -> _;
}
