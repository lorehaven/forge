//! The handlers, and what they all have to do first.

use super::{PathError, Storage};
use actix_web::{HttpResponse, http::StatusCode};
use serde_json::json;
use std::path::PathBuf;

pub mod delete;
pub mod download;
pub mod list;
pub mod upload;

pub fn error(status: StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(json!({ "error": message }))
}

pub fn not_found(message: &str) -> HttpResponse {
    error(StatusCode::NOT_FOUND, message)
}

/// The storage a request names, or the response to send instead.
///
/// A disabled feature answers the same 404 as an unknown storage, deliberately:
/// whether this deployment *could* serve files is not something an unauthorised
/// caller learns by asking.
///
/// Boxed: `HttpResponse` grew past clippy's `result_large_err` threshold, and
/// every caller already only moves the error once, straight into a `return` -
/// boxing it costs one allocation on a path that was about to answer with an
/// HTTP response anyway.
pub fn storage_or_error(name: &str) -> Result<&'static Storage, Box<HttpResponse>> {
    if !crate::routers::files_enabled() {
        return Err(Box::new(not_found("file storage is not enabled")));
    }

    super::storage(name)
        .ok_or_else(|| Box::new(not_found(&format!("no file storage named `{name}`"))))
}

/// The storage and the resolved on-disk path for a `?path=` request.
///
/// Both halves of the check happen here so no handler can accidentally do only
/// the lexical one: [`super::relative`] refuses a path that spells its way out,
/// and [`super::confined`] refuses one that gets out through a symlink.
pub async fn target_or_error(
    storage_name: &str,
    path: &str,
) -> Result<(&'static Storage, PathBuf), Box<HttpResponse>> {
    let storage = storage_or_error(storage_name)?;

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

    Ok((storage, target))
}
