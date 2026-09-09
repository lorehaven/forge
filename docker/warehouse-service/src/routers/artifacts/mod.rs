//! Multi-platform artifact storage, addressed by
//! `{program}/{platform}/{version_code}`.
//!
//! `program` is a reverse-DNS application id shared across platforms
//! (`dev.lorehaven.pedlar` on Android, Linux and Windows alike). Neither the
//! cargo registry nor the docker registry fit an installable app bundle: for
//! Android its identity and version aren't something a publisher types in -
//! they are decoded from the APK's own `AndroidManifest.xml` at publish time
//! (see [`crate::domain::apk_manifest`]), so a caller cannot get a package's
//! catalog entry to say something the archive itself doesn't. For Linux and
//! Windows there is no manifest this service can read, so identity is taken
//! from the publish URL as asserted by the caller (the same `+N` build number
//! the artifact already carries).
//!
//! Versions are immutable once published, same as a crate's tarball - an
//! update is a new `version_code`, not a rewrite - so publishing rejects a
//! `(program, platform, version_code)` that already exists rather than
//! overwriting it.
//!
//! The `/api/v1/apk/*` routes are a thin alias over this module with
//! `platform` forced to `android`, kept so the deployed Pedlar keeps working
//! while it moves to `/api/v1/artifacts` (see [`alias`]).

use crate::domain::artifact::Platform;
use crate::routers::artifact_storage_root;
use actix_web::dev::HttpServiceFactory;
use actix_web::middleware::NormalizePath;
use actix_web::web;
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::actix::middleware::require_write::RequireWrite;
use quench_auth::prelude::JwtConfig;
use std::path::PathBuf;

pub mod alias;
pub mod ops;

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// On-disk directory for one published artifact.
///
/// Layout: `<root>/<program>/<platform>/<version_code>/`
fn artifact_dir(program: &str, platform: Platform, version_code: i64) -> Option<PathBuf> {
    if !validate_program(program) {
        return None;
    }
    Some(
        PathBuf::from(artifact_storage_root())
            .join(program)
            .join(platform.as_str())
            .join(version_code.to_string()),
    )
}

/// On-disk path for a published artifact's bytes. `filename` is stored on the
/// catalog row and echoed back on download, so it is validated here to a
/// single, separator-free path component.
pub fn artifact_file_path(
    program: &str,
    platform: Platform,
    version_code: i64,
    filename: &str,
) -> Option<PathBuf> {
    if !validate_filename(filename) {
        return None;
    }
    Some(artifact_dir(program, platform, version_code)?.join(filename))
}

/// A temporary path an in-flight upload streams to before its identity is
/// checked and it's renamed into place - so a request that fails partway
/// through never leaves a partial file at the real path. The pid+counter
/// suffix (mirroring `routers::files::ops::upload::staging_path`) keeps two
/// concurrent publishes of the same version from sharing one staging file.
pub fn artifact_staging_path(
    program: &str,
    platform: Platform,
    version_code: i64,
    filename: &str,
) -> Option<PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    artifact_file_path(program, platform, version_code, filename).map(|path| {
        path.with_extension(format!(
            "part.{}.{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    })
}

/// Validates a Java-style program identifier: dot-separated segments, each
/// starting with a letter or underscore and continuing with letters, digits,
/// or underscores, ≤255 characters overall. Stricter than crates' charset
/// deliberately - the name becomes a path component, so ruling out
/// `.`-adjacent oddities (`..`, a leading/trailing dot, an empty segment) up
/// front is what keeps [`artifact_dir`] from ever needing to defend against
/// traversal.
pub fn validate_program(name: &str) -> bool {
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

/// A download filename: one path component, no separators, no `..`, no control
/// bytes, ≤255 chars. Kept liberal on the rest (dots, dashes, `+`) so
/// `pedlar-7.tar.gz` and `Mathom Setup 1.2.3.msi` both pass.
pub fn validate_filename(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 || name == "." || name == ".." {
        return false;
    }
    !name
        .chars()
        .any(|c| c == '/' || c == '\\' || c == '\0' || c.is_control())
}

/// The stored filename for a publish that didn't supply one: `apk` gets the
/// historical `<program>-<code>.apk`, everything else `<program>-<code>.<fmt>`.
pub fn default_filename(program: &str, version_code: i64, format: &str) -> String {
    format!("{program}-{version_code}.{format}")
}

// ---------------------------------------------------------------------------
// One-time storage relocation
// ---------------------------------------------------------------------------

/// Moves a pre-multi-platform APK tree from the old
/// `<root>/<program>/<version_code>/` layout to the new
/// `<root>/<program>/android/<version_code>/`, once, on boot.
///
/// Best-effort and idempotent: an already-relocated version (its `android`
/// subdir present) is left alone, and a `rename` that fails (e.g. the new root
/// is on a different filesystem) is logged and skipped rather than fatal - the
/// `0004` data migration has already moved the catalog rows, so the operator
/// can relocate the files by hand if this can't.
pub fn relocate_legacy_apk_storage() {
    let new_root = PathBuf::from(artifact_storage_root());
    let legacy_root = PathBuf::from(envmnt::get_or("APK_STORAGE_PATH", "./storage/apk"));
    if !legacy_root.is_dir() {
        return;
    }

    let Ok(program_entries) = std::fs::read_dir(&legacy_root) else {
        return;
    };
    let program_dirs: Vec<_> = program_entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();

    let mut moved = 0usize;
    for program_entry in program_dirs {
        let program = program_entry.file_name();
        let Ok(code_entries) = std::fs::read_dir(program_entry.path()) else {
            continue;
        };
        let code_dirs: Vec<_> = code_entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .collect();

        for code_entry in code_dirs {
            let code = code_entry.file_name();
            // `android` is the new layout's segment - already relocated.
            if code == "android" || code.to_str().is_none_or(|c| c.parse::<i64>().is_err()) {
                continue;
            }
            let dest = new_root.join(&program).join("android").join(&code);
            if dest.exists() {
                continue;
            }
            if let Some(parent) = dest.parent()
                && std::fs::create_dir_all(parent).is_err()
            {
                tracing::warn!(
                    ?dest,
                    "artifact storage: could not create relocation target"
                );
                continue;
            }
            match std::fs::rename(code_entry.path(), &dest) {
                Ok(()) => moved += 1,
                Err(err) => {
                    tracing::warn!(
                        from = ?code_entry.path(),
                        to = ?dest,
                        %err,
                        "artifact storage: legacy APK relocation failed; move it by hand",
                    );
                }
            }
        }
    }

    if moved > 0 {
        tracing::info!(
            moved,
            "artifact storage: relocated legacy APK versions under <program>/android/"
        );
    }
}

// ---------------------------------------------------------------------------
// Actix scopes
// ---------------------------------------------------------------------------

/// Route order is load-bearing: actix tries a scope's services in
/// registration order and stops at the first path-and-method match; it does
/// not prefer a literal segment over a same-shaped `{version_code}`. So
/// `/{program}/{platform}/latest[/download]` is registered before
/// `/{program}/{platform}/{version_code}[/download]` - reversed, `latest`
/// would be parsed as a `version_code`.
pub fn scope(jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("/api/v1/artifacts")
        .wrap(NormalizePath::trim())
        .wrap(RequireWrite::new(jwt_config.clone()))
        .wrap(Auth::new(jwt_config))
        .service(ops::publish::handle)
        .service(ops::latest::metadata)
        .service(ops::latest::download)
        .service(ops::download::handle)
        .service(ops::metadata::handle)
        .service(ops::list::platform_versions)
        .service(ops::list::program_versions)
        .service(ops::list::catalog)
        .service(ops::yank::handle)
        .service(ops::unyank::handle)
}

/// `/api/v1/apk/*` - the pre-multi-platform shape, `platform` forced to
/// `android`. Same middleware stack; handlers live in [`alias`].
pub fn apk_alias_scope(jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("/api/v1/apk")
        .wrap(NormalizePath::trim())
        .wrap(RequireWrite::new(jwt_config.clone()))
        .wrap(Auth::new(jwt_config))
        .service(alias::publish)
        .service(alias::latest_metadata)
        .service(alias::latest_download)
        .service(alias::download)
        .service(alias::metadata)
        .service(alias::versions)
        .service(alias::catalog)
        .service(alias::yank)
        .service(alias::unyank)
}
