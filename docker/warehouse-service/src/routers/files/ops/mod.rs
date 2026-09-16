//! The handlers, and what they all have to do first.

use super::{PathError, Storage};
use crate::domain::storage::DynamicStorage;
use crate::routers::files::authz;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_db::prelude::Db;
use quench_http::prelude::{Response, http::StatusCode};
use serde_json::json;
use std::path::PathBuf;

pub mod delete;
pub mod download;
pub mod list;
pub mod storages;
pub mod upload;

pub fn register_routes() {
    delete::register_routes();
    download::register_routes();
    list::register_routes();
    storages::register_routes();
    upload::register_routes();
}

pub fn error(status: StatusCode, message: &str) -> Response {
    Response::json(status, &json!({ "error": message })).unwrap_or_else(|_| Response::new(status))
}

pub fn not_found(message: &str) -> Response {
    error(StatusCode::NOT_FOUND, message)
}

pub fn forbidden(message: &str) -> Response {
    error(StatusCode::FORBIDDEN, message)
}

/// Which kind of storage a name resolved to - env-configured, or database-backed.
pub enum ResolvedStorage {
    Static(&'static Storage),
    Dynamic(DynamicStorage),
}

/// The storage a request names, or the response to send instead. Dynamic
/// (database) tried first, then static (`FILE_STORAGES`); a disabled feature 404s like an unknown name.
pub async fn resolve_storage(db: &Db, name: &str) -> Result<ResolvedStorage, Response> {
    if !crate::routers::files_enabled() {
        return Err(not_found("file storage is not enabled"));
    }

    if let Ok(Some(storage)) = crate::domain::storage::read(db, name).await {
        return Ok(ResolvedStorage::Dynamic(storage));
    }

    super::storage(name)
        .map(ResolvedStorage::Static)
        .ok_or_else(|| not_found(&format!("no file storage named `{name}`")))
}

/// Static storages: read is open, write needs the blanket `warehouse:write`
/// grant. Dynamic storages: both gated via `authz::can_on_storage`.
pub fn authorize(
    claims: Option<&Claims>,
    config: &JwtConfig,
    resolved: &ResolvedStorage,
    action: &str,
) -> bool {
    match resolved {
        ResolvedStorage::Static(_) => {
            action != "write" || authz::has_blanket(claims, config, "write")
        }
        ResolvedStorage::Dynamic(storage) => authz::can_on_storage(claims, config, storage, action),
    }
}

/// Resolved on-disk path for a `?path=` request against a *static* storage -
/// [`super::relative`] and [`super::confined`] both run so neither check can be skipped.
pub async fn static_target_or_error(
    storage: &'static Storage,
    path: &str,
) -> Result<PathBuf, Response> {
    let target = super::resolve(storage, path).map_err(|why| {
        let status = match why {
            PathError::Empty => StatusCode::BAD_REQUEST,
            _ => StatusCode::FORBIDDEN,
        };
        error(status, why.message())
    })?;

    if !super::confined(&storage.root, &target).await {
        tracing::warn!(
            "refused `{path}` in storage `{}`: resolves outside the storage root",
            storage.name
        );
        return Err(error(
            StatusCode::FORBIDDEN,
            "path resolves outside the storage",
        ));
    }

    Ok(target)
}

/// Lexically validates `?path=` for a *dynamic* storage (same rules as
/// `super::relative`), normalised to the `storage_files` key form - no filesystem check needed.
pub fn dynamic_path(path: &str) -> Result<String, Response> {
    let relative = super::relative(path).map_err(|why| {
        let status = match why {
            PathError::Empty => StatusCode::BAD_REQUEST,
            _ => StatusCode::FORBIDDEN,
        };
        error(status, why.message())
    })?;

    Ok(relative.to_string_lossy().into_owned())
}
