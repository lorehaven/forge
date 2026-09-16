//! Dynamic storage administration (`POST`/`PATCH`/`DELETE`) and its sync feed
//! (`GET .../sync`). Provisioning is admin-only; see `authz` for ownership rules after that.

use super::{ResolvedStorage, authorize, error, forbidden, not_found, resolve_storage};
use crate::domain::storage::{self, NewStorage, StorageUpdate};
use crate::domain::storage_file;
use crate::routers::files::OptionalClaims;
use crate::routers::files::authz;
use crate::routers::files::dynamic;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Inject, Json, Path, Query, Response, delete, get, http::StatusCode, patch, post,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateStorage {
    pub name: String,
    pub owner: String,
    #[serde(default)]
    pub max_file_bytes: Option<i64>,
    #[serde(default)]
    pub quota_bytes: Option<i64>,
    #[serde(default)]
    pub sync_enabled: bool,
}

#[post("/api/v1/files")]
#[tracing::instrument(skip(claims, config, db, body))]
pub async fn create(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Json(body): Json<CreateStorage>,
) -> Response {
    if !crate::routers::files_enabled() {
        return not_found("file storage is not enabled");
    }

    if !authz::has_blanket(claims.as_ref(), &config, "write") {
        return forbidden("write access to warehouse is required to provision a storage");
    }

    if !crate::routers::files::valid_storage_name(&body.name) {
        return error(
            StatusCode::BAD_REQUEST,
            "storage names may use letters, digits, `-` and `_` only",
        );
    }

    if crate::routers::files::storage(&body.name).is_some() {
        return error(
            StatusCode::CONFLICT,
            "a static storage already uses that name",
        );
    }

    let new = NewStorage {
        name: body.name.clone(),
        owner: body.owner.clone(),
        max_file_bytes: body.max_file_bytes,
        quota_bytes: body
            .quota_bytes
            .unwrap_or_else(dynamic::default_quota_bytes),
        sync_enabled: body.sync_enabled,
    };

    match storage::create(&db, &new).await {
        Ok(storage) => Response::json(StatusCode::CREATED, &storage)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(problem) if problem.is_unique_violation() => error(
            StatusCode::CONFLICT,
            "a storage with that name already exists",
        ),
        Err(problem) if problem.is_foreign_key_violation() => {
            error(StatusCode::BAD_REQUEST, "no such user to own this storage")
        }
        Err(problem) => {
            tracing::error!("creating dynamic storage failed: {problem}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not create the storage",
            )
        }
    }
}

#[derive(Deserialize)]
pub struct PatchStorage {
    #[serde(default)]
    pub max_file_bytes: Option<i64>,
    /// Clears `max_file_bytes` back to the default - JSON can't distinguish "leave alone" from "null".
    #[serde(default)]
    pub clear_max_file_bytes: bool,
    #[serde(default)]
    pub quota_bytes: Option<i64>,
    #[serde(default)]
    pub sync_enabled: Option<bool>,
}

#[patch("/api/v1/files/{storage}")]
#[tracing::instrument(skip(claims, config, db, body))]
pub async fn patch(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Path(storage_name): Path<String>,
    Json(body): Json<PatchStorage>,
) -> Response {
    if !authz::has_blanket(claims.as_ref(), &config, "write") {
        return forbidden("write access to warehouse is required to reconfigure a storage");
    }

    let max_file_bytes = if body.clear_max_file_bytes {
        Some(None)
    } else {
        body.max_file_bytes.map(Some)
    };

    let changes = StorageUpdate {
        max_file_bytes,
        quota_bytes: body.quota_bytes,
        sync_enabled: body.sync_enabled,
    };

    match storage::update(&db, &storage_name, &changes).await {
        Ok(Some(storage)) => Response::json(StatusCode::OK, &storage)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Ok(None) => not_found("no such dynamic storage"),
        Err(problem) => {
            tracing::error!("updating dynamic storage failed: {problem}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not update the storage",
            )
        }
    }
}

#[delete("/api/v1/files/{storage}")]
#[tracing::instrument(skip(claims, config, db))]
pub async fn remove(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Path(storage_name): Path<String>,
) -> Response {
    if !authz::has_blanket(claims.as_ref(), &config, "write") {
        return forbidden("write access to warehouse is required to delete a storage");
    }

    let Ok(Some(storage)) = storage::read(&db, &storage_name).await else {
        return not_found("no such dynamic storage");
    };

    let Some(root) = dynamic::root() else {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "this deployment has no DYNAMIC_STORAGE_ROOT configured",
        );
    };

    let files = match storage_file::list_files(&db, &storage.name, "").await {
        Ok(files) => files,
        Err(problem) => {
            tracing::error!("listing dynamic storage before delete failed: {problem}");
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not delete the storage",
            );
        }
    };

    for file in files {
        let _ = storage_file::delete_file(&db, &storage.name, &file.path, |sha256| {
            dynamic::blob_path(&root, sha256)
        })
        .await;
    }

    match storage::delete(&db, &storage.name).await {
        Ok(true) => Response::new(StatusCode::NO_CONTENT),
        Ok(false) => not_found("no such dynamic storage"),
        Err(problem) => {
            tracing::error!("deleting dynamic storage failed: {problem}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not delete the storage",
            )
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SyncQuery {
    #[serde(default)]
    pub since: i64,
}

/// `GET /api/v1/files/{storage}/sync?since=<id>` - change feed since a checkpoint.
#[get("/api/v1/files/{storage}/sync")]
#[tracing::instrument(skip(claims, config, db))]
pub async fn sync_log(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Path(storage_name): Path<String>,
    Query(query): Query<SyncQuery>,
) -> Response {
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };

    if !authorize(claims.as_ref(), &config, &resolved, "read") {
        return forbidden("read access to this storage is required");
    }

    let ResolvedStorage::Dynamic(storage) = resolved else {
        return error(StatusCode::CONFLICT, "static storages have no sync log");
    };

    if !storage.sync_enabled {
        return error(StatusCode::CONFLICT, "sync is not enabled for this storage");
    }

    match storage_file::sync_log_since(&db, &storage.name, query.since).await {
        Ok(entries) => Response::json(StatusCode::OK, &entries)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(problem) => {
            tracing::error!("reading sync log failed: {problem}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the sync log",
            )
        }
    }
}

pub fn register_routes() {
    let _ = create as fn(_, _, _, _) -> _;
    let _ = patch as fn(_, _, _, _, _) -> _;
    let _ = remove as fn(_, _, _, _) -> _;
    let _ = sync_log as fn(_, _, _, _, _) -> _;
}
