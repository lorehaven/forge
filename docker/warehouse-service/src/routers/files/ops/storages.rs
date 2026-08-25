//! Dynamic storage administration - `POST`/`PATCH`/`DELETE
//! /api/v1/files/{storage}` provision, reconfigure and remove one - and its
//! sync change feed, `GET /api/v1/files/{storage}/sync`.
//!
//! Provisioning is deliberately admin-only (the blanket `warehouse:write`
//! grant, via `authz::has_blanket`), not owner-or-scoped: an eventual owner
//! does not get to create their own storage or change their own quota. See
//! `authz`'s docs for the owner-or-scoped rule that governs everything else
//! about a dynamic storage once it exists.

use super::{ResolvedStorage, authorize, error, forbidden, not_found, resolve_storage};
use crate::domain::storage::{self, NewStorage, StorageUpdate};
use crate::domain::storage_file;
use crate::routers::files::authz;
use crate::routers::files::dynamic;
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, patch, post, web};
use quench_db::prelude::Db;
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

#[post("")]
#[tracing::instrument(skip(request, body))]
pub async fn create(
    request: HttpRequest,
    db: web::Data<Db>,
    body: web::Json<CreateStorage>,
) -> impl Responder {
    if !crate::routers::files_enabled() {
        return not_found("file storage is not enabled");
    }

    if !authz::has_blanket(&request, "write") {
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
        Ok(storage) => HttpResponse::Created().json(storage),
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
    /// Clears `max_file_bytes` back to the deployment default. A plain
    /// `Option<i64>` field can't tell "leave alone" from "set to null" apart
    /// once JSON strips the distinction, so clearing it is its own flag.
    #[serde(default)]
    pub clear_max_file_bytes: bool,
    #[serde(default)]
    pub quota_bytes: Option<i64>,
    #[serde(default)]
    pub sync_enabled: Option<bool>,
}

#[patch("/{storage}")]
#[tracing::instrument(skip(request, body))]
pub async fn patch(
    request: HttpRequest,
    db: web::Data<Db>,
    storage_name: web::Path<String>,
    body: web::Json<PatchStorage>,
) -> impl Responder {
    if !authz::has_blanket(&request, "write") {
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
        Ok(Some(storage)) => HttpResponse::Ok().json(storage),
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

#[delete("/{storage}")]
#[tracing::instrument(skip(request))]
pub async fn remove(
    request: HttpRequest,
    db: web::Data<Db>,
    storage_name: web::Path<String>,
) -> impl Responder {
    if !authz::has_blanket(&request, "write") {
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
        Ok(true) => HttpResponse::NoContent().finish(),
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

/// `GET /api/v1/files/{storage}/sync?since=<id>` - the change feed for a
/// `sync_enabled` storage, so a backup client can ask "what changed since my
/// last checkpoint" instead of re-listing and re-hashing everything it
/// already sent.
#[get("/{storage}/sync")]
#[tracing::instrument(skip(request))]
pub async fn sync_log(
    request: HttpRequest,
    db: web::Data<Db>,
    storage_name: web::Path<String>,
    query: web::Query<SyncQuery>,
) -> impl Responder {
    let resolved = match resolve_storage(&db, &storage_name).await {
        Ok(resolved) => resolved,
        Err(response) => return *response,
    };

    if !authorize(&request, &resolved, "read") {
        return forbidden("read access to this storage is required");
    }

    let ResolvedStorage::Dynamic(storage) = resolved else {
        return error(StatusCode::CONFLICT, "static storages have no sync log");
    };

    if !storage.sync_enabled {
        return error(StatusCode::CONFLICT, "sync is not enabled for this storage");
    }

    match storage_file::sync_log_since(&db, &storage.name, query.since).await {
        Ok(entries) => HttpResponse::Ok().json(entries),
        Err(problem) => {
            tracing::error!("reading sync log failed: {problem}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the sync log",
            )
        }
    }
}
