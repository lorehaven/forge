//! Filesystem layout for dynamic (DB-backed) storages.
//!
//! A dynamic storage has no directory of its own the way a static one does -
//! its content is addressed by `crate::domain::storage_file` through the
//! database, and every storage's bytes live in one shared, content-addressed
//! blob store under [`root`]. This module only knows that layout; the
//! decisions about which blob a path resolves to, dedup, quota and the sync
//! log all live in `crate::domain`.

use std::path::{Path, PathBuf};

/// The blob store's root, or `None` if this deployment has no dynamic
/// storages configured. Unlike `FILE_STORAGES`, there is exactly one root: a
/// dynamic storage's name is a database key, not something that picks its own
/// directory, so there is nothing for a second root to disambiguate.
pub fn root() -> Option<PathBuf> {
    let raw = envmnt::get_or("DYNAMIC_STORAGE_ROOT", "");
    if raw.trim().is_empty() {
        None
    } else {
        Some(PathBuf::from(raw))
    }
}

/// The quota a newly created storage gets when its admin doesn't name one -
/// 10 GiB, generous enough for a phone's worth of photos without an admin
/// having to think about it for the common case.
pub fn default_quota_bytes() -> i64 {
    let loader = quench_config::ConfigLoader::new("WAREHOUSE");
    loader.env_u64("DEFAULT_STORAGE_QUOTA_BYTES", 10 * 1024 * 1024 * 1024) as i64
}

/// Where a blob's content lives: content-addressed and sharded two levels
/// deep (`ab/cd/abcd...`) so `.blobs` itself never ends up with millions of
/// entries in one directory, the same concern Docker's own blob storage in
/// this service already has an answer for.
pub fn blob_path(root: &Path, sha256: &str) -> PathBuf {
    let mut path = root.join(".blobs");
    if let Some(a) = sha256.get(0..2) {
        path.push(a);
    }
    if let Some(b) = sha256.get(2..4) {
        path.push(b);
    }
    path.push(sha256);
    path
}

/// A fresh, collision-free path to stream an upload's bytes into before its
/// digest is known and it can be placed (or discarded, on a dedup hit).
pub fn staging_path(root: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let unique = format!(
        "{}.{}.part",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );

    root.join(".tmp").join(unique)
}
