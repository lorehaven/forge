//! APK storage, addressed by package name and Android's own `versionCode`.
//!
//! Neither the cargo registry nor the docker registry fit an Android app:
//! its identity and version aren't something a publisher types in - they are
//! decoded from the APK's own `AndroidManifest.xml` at publish time (see
//! [`crate::domain::apk_manifest`]), so a caller cannot get a package's
//! catalog entry to say something the archive itself doesn't. That is the
//! property the eventual Android app-store backend needs: it can trust
//! `GET /api/v1/apk` without also re-parsing every APK it lists.
//!
//! Versions are immutable once published, same as a crate's tarball - an
//! update is a new `versionCode`, not a rewrite - so publishing rejects a
//! `versionCode` that already exists rather than overwriting it.

use crate::routers::apk_storage_root;
use actix_web::dev::HttpServiceFactory;
use actix_web::middleware::NormalizePath;
use actix_web::web;
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::actix::middleware::require_write::RequireWrite;
use quench_auth::prelude::JwtConfig;
use std::path::PathBuf;

pub mod ops;

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// On-disk path for a published APK.
///
/// Layout: `<root>/<package_name>/<version_code>/<package_name>-<version_code>.apk`
pub fn apk_file_path(package_name: &str, version_code: i64) -> Option<PathBuf> {
    if !validate_package_name(package_name) {
        return None;
    }
    Some(
        PathBuf::from(apk_storage_root())
            .join(package_name)
            .join(version_code.to_string())
            .join(format!("{package_name}-{version_code}.apk")),
    )
}

/// A temporary path an in-flight upload streams to before its manifest is
/// checked and it's renamed into place - so a request that fails partway
/// through never leaves a partial file at the real path. The pid+counter
/// suffix (mirroring `routers::files::ops::upload::staging_path`) keeps two
/// concurrent publishes - of the same version, or racing a stale `.part`
/// left by a crashed process - from sharing one staging file and
/// interleaving their bytes.
pub fn apk_staging_path(package_name: &str, version_code: i64) -> Option<PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    apk_file_path(package_name, version_code).map(|path| {
        path.with_extension(format!(
            "apk.{}.{}.part",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    })
}

/// Validates a Java-style package identifier: dot-separated segments, each
/// starting with a letter or underscore and continuing with letters, digits,
/// or underscores, ≤255 characters overall. This is stricter than crates'
/// charset deliberately - the name becomes two path components
/// (`<package_name>/<version_code>/...`), so ruling out `.`-adjacent
/// oddities (`..`, a leading/trailing dot, an empty segment) up front is what
/// keeps [`apk_file_path`] from ever needing to defend against traversal.
pub fn validate_package_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }

    name.split('.').all(|segment| {
        let mut chars = segment.chars();
        match chars.next() {
            Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
            _ => return false,
        }
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

// ---------------------------------------------------------------------------
// Actix scope
// ---------------------------------------------------------------------------

/// Route order is load-bearing: actix tries a scope's services in
/// registration order and stops at the first path-and-method match, it does
/// not prefer a literal segment over a same-shaped `{version_code}` on its
/// own. `/{package}/latest` and `/{package}/latest/download` are therefore
/// registered before `/{package}/{version_code}` and
/// `/{package}/{version_code}/download` - reversed, `latest` would be parsed
/// as a `version_code` and never reach the routes below it.
pub fn scope(jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("/api/v1/apk")
        .wrap(NormalizePath::trim())
        .wrap(RequireWrite::new(jwt_config.clone()))
        .wrap(Auth::new(jwt_config))
        .service(ops::publish::handle)
        .service(ops::latest::metadata)
        .service(ops::latest::download)
        .service(ops::download::handle)
        .service(ops::metadata::handle)
        .service(ops::list::versions)
        .service(ops::list::catalog)
        .service(ops::yank::handle)
        .service(ops::unyank::handle)
}
