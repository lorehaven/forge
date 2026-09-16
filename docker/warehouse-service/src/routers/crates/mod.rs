use crate::routers::crates_storage_root;
use std::path::PathBuf;

pub mod index;
pub mod ops;
pub mod owners;
pub mod search;

/// On-disk path for a `.crate` tarball: `<root>/<n>/<version>/<n>-<version>.crate`.
pub fn crate_file_path(name: &str, version: &str) -> Option<PathBuf> {
    if !validate_crate_name(name) || !validate_version(version) {
        return None;
    }
    Some(
        PathBuf::from(crates_storage_root())
            .join(name)
            .join(version)
            .join(format!("{name}-{version}.crate")),
    )
}

/// On-disk path for the sparse index file: `<root>/index/<prefix>/<n>`.
pub fn index_file_path(name: &str) -> Option<PathBuf> {
    if !validate_crate_name(name) {
        return None;
    }
    let prefix = index_prefix(name);
    Some(
        PathBuf::from(crates_storage_root())
            .join("index")
            .join(&prefix)
            .join(name),
    )
}

/// Sparse-index directory prefix, following the crates.io convention.
pub fn index_prefix(name: &str) -> String {
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
pub fn validate_crate_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    name.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Validates a semver-ish version string: non-empty, ≤64 chars, safe characters only.
pub fn validate_version(version: &str) -> bool {
    if version.is_empty() || version.len() > 64 {
        return false;
    }
    version
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
}

pub fn register_routes() {
    search::register_routes();
    ops::register_routes();
    owners::register_routes();
    index::register_routes();
}
