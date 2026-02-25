use crate::routers::CRATES_STORAGE_ROOT;
use actix_web::dev::HttpServiceFactory;
use actix_web::middleware::NormalizePath;
use actix_web::web;
use ops::{download, publish, unyank, yank};
use std::path::PathBuf;
use utoipa::OpenApi;

pub mod index;
pub mod ops;
pub mod owners;
pub mod search;

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// On-disk path for a `.crate` tarball.
///
/// Layout: `<root>/<n>/<version>/<n>-<version>.crate`
pub(super) fn crate_file_path(name: &str, version: &str) -> Option<PathBuf> {
    if !validate_crate_name(name) || !validate_version(version) {
        return None;
    }
    Some(
        PathBuf::from(CRATES_STORAGE_ROOT.as_str())
            .join(name)
            .join(version)
            .join(format!("{name}-{version}.crate")),
    )
}

/// On-disk path for the newline-delimited JSON sparse index file.
///
/// Layout: `<root>/index/<prefix>/<n>`
pub(super) fn index_file_path(name: &str) -> Option<PathBuf> {
    if !validate_crate_name(name) {
        return None;
    }
    let prefix = index_prefix(name);
    Some(
        PathBuf::from(CRATES_STORAGE_ROOT.as_str())
            .join("index")
            .join(&prefix)
            .join(name),
    )
}

/// Sparse-index directory prefix following the crates.io convention:
/// - 1 char  → `1`
/// - 2 chars → `2`
/// - 3 chars → `3/<first_char>`
/// - 4+ chars → `<first_two>/<second_two>`
pub(crate) fn index_prefix(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match lower.len() {
        0 => String::new(),
        1 => "1".to_string(),
        2 => "2".to_string(),
        3 => format!("3/{}", &lower[..1]),
        _ => format!("{}/{}", &lower[..2], &lower[2..4]),
    }
}

/// Validates a crate name: non-empty, ≤64 chars, ASCII alphanumeric / `-` / `_`.
pub(super) fn validate_crate_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    name.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Validates a semver-ish version string: non-empty, ≤64 chars, safe characters only.
pub(super) fn validate_version(version: &str) -> bool {
    if version.is_empty() || version.len() > 64 {
        return false;
    }
    version
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
}

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    paths(
        publish::handle,
        download::handle,
        yank::handle,
        unyank::handle,
        search::handle,
        owners::list,
        owners::add,
        owners::remove,
    ),
    tags(
        (name = "crates", description = "Crate publish, yank, and download endpoints"),
        (name = "crates - search", description = "Search endpoint"),
        (name = "crates - owners", description = "Crate ownership management"),
    )
)]
pub struct CratesApiDoc;

#[derive(OpenApi)]
#[openapi(
    paths(
        index::get_index_config,
        index::get_crate_index,
    ),
    tags(
        (name = "crates - index",  description = "Sparse registry index (cargo sparse protocol)"),
    )
)]
pub struct CratesIndexApiDoc;

// ---------------------------------------------------------------------------
// Actix scope
// ---------------------------------------------------------------------------

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/api/v1/crates")
        .wrap(NormalizePath::trim())
        .service(search::handle)
        .service(publish::handle)
        .service(download::handle)
        .service(owners::list)
        .service(owners::add)
        .service(owners::remove)
}

pub fn scope_index() -> impl HttpServiceFactory {
    web::scope("/index")
        .wrap(NormalizePath::trim())
        .service(index::get_index_config)
        .service(index::get_crate_index)
}
