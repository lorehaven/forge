//! Filesystem layout for dynamic (DB-backed) storages: one shared,
//! content-addressed blob store under [`root`]; `crate::domain` owns the rest.

use std::path::{Path, PathBuf};

/// The blob store's root, or `None` if no dynamic storages are configured.
pub fn root() -> Option<PathBuf> {
    let raw = envmnt::get_or("DYNAMIC_STORAGE_ROOT", "");
    if raw.trim().is_empty() {
        None
    } else {
        Some(PathBuf::from(raw))
    }
}

/// Default quota for a newly created storage: 10 GiB, a phone backup's worth.
pub fn default_quota_bytes() -> i64 {
    let loader = quench_config::ConfigLoader::new("WAREHOUSE");
    loader.env_u64("DEFAULT_STORAGE_QUOTA_BYTES", 10 * 1024 * 1024 * 1024) as i64
}

/// Content-addressed path, sharded two levels deep so `.blobs` doesn't fill with millions of entries.
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
