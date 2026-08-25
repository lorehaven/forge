//! The handlers, and what they all have to do first.

use super::{PathError, Storage};
use crate::domain::storage::DynamicStorage;
use crate::routers::files::authz;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, http::StatusCode};
use quench_db::prelude::Db;
use serde_json::json;
use std::path::PathBuf;

pub mod delete;
pub mod download;
pub mod list;
pub mod storages;
pub mod upload;

pub fn error(status: StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(json!({ "error": message }))
}

pub fn not_found(message: &str) -> HttpResponse {
    error(StatusCode::NOT_FOUND, message)
}

pub fn forbidden(message: &str) -> HttpResponse {
    error(StatusCode::FORBIDDEN, message)
}

/// Which kind of storage a name resolved to - a legacy, env-configured
/// directory, or a database-backed one with an owner, a quota and (maybe)
/// dedup's blob store behind it.
pub enum ResolvedStorage {
    Static(&'static Storage),
    Dynamic(DynamicStorage),
}

/// The storage a request names, or the response to send instead.
///
/// Dynamic storages (the database) are tried first, static ones (`FILE_STORAGES`)
/// second - a name cannot be both, rejected as a conflict when a dynamic
/// storage is created (see `ops::storages::create`). A database error (an
/// in-memory test database, say) falls back to the static lookup rather than
/// failing the request: a deployment with no Postgres configured simply has
/// no dynamic storages, the same as one with `DYNAMIC_STORAGE_ROOT` unset.
///
/// A disabled feature answers the same 404 as an unknown storage, deliberately:
/// whether this deployment *could* serve files is not something an unauthorised
/// caller learns by asking.
pub async fn resolve_storage(db: &Db, name: &str) -> Result<ResolvedStorage, Box<HttpResponse>> {
    if !crate::routers::files_enabled() {
        return Err(Box::new(not_found("file storage is not enabled")));
    }

    if let Ok(Some(storage)) = crate::domain::storage::read(db, name).await {
        return Ok(ResolvedStorage::Dynamic(storage));
    }

    super::storage(name)
        .map(ResolvedStorage::Static)
        .ok_or_else(|| Box::new(not_found(&format!("no file storage named `{name}`"))))
}

/// Whether the caller behind `request` may perform `action` ("read" or
/// "write") on `resolved`.
///
/// A static storage keeps its pre-dynamic-storage behaviour exactly: read is
/// open to anyone with a valid realm identity for this service (there never
/// was a per-action check on the safe methods - only `RequireWrite`'s
/// mutating-method gate, reproduced here as the blanket `warehouse:write`
/// check), write needs the blanket grant. A dynamic storage is gated on both
/// read and write, through `authz::can_on_storage` - see that module's docs
/// for why its default is private-unless-shared rather than blanket-first.
pub fn authorize(request: &HttpRequest, resolved: &ResolvedStorage, action: &str) -> bool {
    match resolved {
        ResolvedStorage::Static(_) => action != "write" || authz::has_blanket(request, "write"),
        ResolvedStorage::Dynamic(storage) => authz::can_on_storage(request, storage, action),
    }
}

/// The storage and the resolved on-disk path for a `?path=` request against a
/// *static* storage. Dynamic storages resolve through
/// `crate::domain::storage_file` instead - there is no filesystem path to
/// confine, since a dynamic storage's content is addressed by digest, not by
/// where a caller's path happens to land.
///
/// Both halves of the check happen here so no handler can accidentally do only
/// the lexical one: [`super::relative`] refuses a path that spells its way out,
/// and [`super::confined`] refuses one that gets out through a symlink.
pub async fn static_target_or_error(
    storage: &'static Storage,
    path: &str,
) -> Result<PathBuf, Box<HttpResponse>> {
    let target = super::resolve(storage, path).map_err(|why| {
        // A refused path is the caller's mistake to fix, so it says which rule
        // it broke - none of which tells them anything about the host.
        let status = match why {
            PathError::Empty => StatusCode::BAD_REQUEST,
            _ => StatusCode::FORBIDDEN,
        };
        Box::new(error(status, why.message()))
    })?;

    if !super::confined(&storage.root, &target).await {
        tracing::warn!(
            "refused `{path}` in storage `{}`: resolves outside the storage root",
            storage.name
        );
        return Err(Box::new(error(
            StatusCode::FORBIDDEN,
            "path resolves outside the storage",
        )));
    }

    Ok(target)
}

/// Lexically validates a caller's `?path=` for a *dynamic* storage - the same
/// rules `super::relative` applies to a static storage's path (no `..`, no
/// absolute path, no control bytes), normalised to the string form used as
/// the `storage_files` key. There is no filesystem confinement check to make
/// here: the result is a database key, resolved to a blob by digest, not a
/// path a symlink could redirect.
pub fn dynamic_path(path: &str) -> Result<String, Box<HttpResponse>> {
    let relative = super::relative(path).map_err(|why| {
        let status = match why {
            PathError::Empty => StatusCode::BAD_REQUEST,
            _ => StatusCode::FORBIDDEN,
        };
        Box::new(error(status, why.message()))
    })?;

    Ok(relative.to_string_lossy().into_owned())
}

/// The claims on `request`, if any - dynamic handlers that need to know
/// *who* is asking (not just whether they may) read this directly rather
/// than going back through `authz`.
pub fn claims(request: &HttpRequest) -> Option<quench_auth::prelude::Claims> {
    request
        .extensions()
        .get::<quench_auth::prelude::Claims>()
        .cloned()
}
