//! Rivet package storage, addressed by `{name}/{version}` (semver, build metadata allowed).
//! Unlike artifacts, identity is never taken from the URL alone: the archive's own `rivet.toml`
//! must agree with it.

use crate::routers::rivet_storage_root;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_auth::http::middleware::require_write::RequireWrite;
use quench_http::prelude::{Endpoint, OnPathPrefix, wrap};
use std::path::PathBuf;
use std::sync::Arc;

pub mod ops;

/// On-disk directory for one package version: `<root>/<name>/<version>/`.
/// `None` unless both parts are valid, which is what keeps either from traversing a path.
fn package_dir(name: &str, version: &str) -> Option<PathBuf> {
    if !valid_name(name) || !valid_version(version) {
        return None;
    }
    Some(PathBuf::from(rivet_storage_root()).join(name).join(version))
}

/// On-disk path for a published package's bytes.
pub fn package_file_path(name: &str, version: &str) -> Option<PathBuf> {
    Some(package_dir(name, version)?.join(rivet_package::file_name(name, version)))
}

/// Where an in-flight upload streams before its contents are checked and it's moved into place.
/// The pid+counter suffix keeps two concurrent publishes of the same version from colliding.
pub fn package_staging_path(name: &str, version: &str) -> Option<PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    package_file_path(name, version).map(|path| {
        path.with_extension(format!(
            "part.{}.{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    })
}

/// A DNS-1123 label, the same rule the manifest enforces.
pub fn valid_name(name: &str) -> bool {
    rivet_package::manifest::is_valid_name(name)
}

/// Semver; `semver` rejects separators, so a valid version is also a safe path component.
pub fn valid_version(version: &str) -> bool {
    semver::Version::parse(version).is_ok()
}

/// `Auth` + `RequireWrite` over `/api/v1/rivets`: reads need any valid realm identity, and the
/// `PUT`/`DELETE` writes need the blanket `warehouse:write` grant.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/api/v1/rivets").into_boxed_str());

    let app = wrap(
        app,
        OnPathPrefix::new(prefix, RequireWrite::new(jwt_config.clone())),
    );
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}

pub fn register_routes() {
    ops::register_routes();
}
