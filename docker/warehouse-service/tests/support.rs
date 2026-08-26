//! Shared locks for tests that redirect this crate's process-global storage
//! env vars at a tempdir. Each covers a distinct set of env vars, so tests
//! touching unrelated vars don't serialize against each other.
#![allow(dead_code)]

use std::sync::{Mutex, OnceLock};

/// Guards `CRATES_STORAGE_PATH`/`STORAGE_PATH`/`APK_STORAGE_PATH`, each read
/// fresh on every call by
/// `warehouse_service::routers::crates_storage_root`/`docker_storage_root`/
/// `apk_storage_root` - two tests setting different values concurrently
/// would otherwise race each other's storage roots out from under them.
pub fn storage_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Guards the docker registry token signing secret env var.
pub fn secret_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Guards whatever env var controls blob-retrieve redirect behavior.
pub fn redirect_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Points `STORAGE_PATH` (the docker registry's storage root) at a fresh
/// tempdir for the duration of one test, holding `storage_env_lock` so
/// concurrent tests doing the same thing don't race each other's roots.
pub struct WithDockerStorageRoot {
    _guard: std::sync::MutexGuard<'static, ()>,
    pub dir: tempfile::TempDir,
}

impl Default for WithDockerStorageRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl WithDockerStorageRoot {
    pub fn new() -> Self {
        let guard = storage_env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::tempdir().expect("tempdir");
        envmnt::set("STORAGE_PATH", dir.path().to_str().unwrap());
        Self { _guard: guard, dir }
    }
}

impl Drop for WithDockerStorageRoot {
    fn drop(&mut self) {
        envmnt::remove("STORAGE_PATH");
    }
}

/// Points `CRATES_STORAGE_PATH` (the cargo registry's storage root) at a
/// fresh tempdir for the duration of one test, holding `storage_env_lock` so
/// concurrent tests doing the same thing don't race each other's roots.
pub struct WithCratesStorageRoot {
    _guard: std::sync::MutexGuard<'static, ()>,
    pub dir: tempfile::TempDir,
}

impl Default for WithCratesStorageRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl WithCratesStorageRoot {
    pub fn new() -> Self {
        let guard = storage_env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::tempdir().expect("tempdir");
        envmnt::set("CRATES_STORAGE_PATH", dir.path().to_str().unwrap());
        Self { _guard: guard, dir }
    }
}

impl Drop for WithCratesStorageRoot {
    fn drop(&mut self) {
        envmnt::remove("CRATES_STORAGE_PATH");
    }
}

/// Points `APK_STORAGE_PATH` (the apk registry's storage root) at a fresh
/// tempdir for the duration of one test, holding `storage_env_lock` so
/// concurrent tests doing the same thing don't race each other's roots.
pub struct WithApkStorageRoot {
    _guard: std::sync::MutexGuard<'static, ()>,
    pub dir: tempfile::TempDir,
}

impl Default for WithApkStorageRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl WithApkStorageRoot {
    pub fn new() -> Self {
        let guard = storage_env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::tempdir().expect("tempdir");
        envmnt::set("APK_STORAGE_PATH", dir.path().to_str().unwrap());
        Self { _guard: guard, dir }
    }
}

impl Drop for WithApkStorageRoot {
    fn drop(&mut self) {
        envmnt::remove("APK_STORAGE_PATH");
    }
}
