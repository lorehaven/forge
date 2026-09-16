//! Multi-platform artifact storage, addressed by `{program}/{platform}/{version_code}`.
//! Android identity is decoded from the APK manifest; other platforms trust the publish URL.

use crate::domain::artifact::Platform;
use crate::routers::artifact_storage_root;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_auth::http::middleware::require_write::RequireWrite;
use quench_http::prelude::{Endpoint, OnPathPrefix, wrap};
use std::path::PathBuf;
use std::sync::Arc;

pub mod alias;
pub mod ops;

/// On-disk directory for one published artifact: `<root>/<program>/<platform>/<version_code>/`.
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

/// On-disk path for a published artifact's bytes; `filename` is validated to one path component.
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

/// Where an in-flight upload streams before its identity is checked and it's renamed into place.
/// The pid+counter suffix keeps two concurrent publishes of the same version from colliding.
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

/// A Java-style dot-separated identifier, ≤255 chars - strict enough that it can't traverse
/// a path once used as one.
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

/// One path component, no separators/`..`/control bytes, ≤255 chars; liberal otherwise.
pub fn validate_filename(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 || name == "." || name == ".." {
        return false;
    }
    !name
        .chars()
        .any(|c| c == '/' || c == '\\' || c == '\0' || c.is_control())
}

/// Stored filename when a publish didn't supply one.
pub fn default_filename(program: &str, version_code: i64, format: &str) -> String {
    format!("{program}-{version_code}.{format}")
}

/// One-time, best-effort move of the pre-multi-platform APK layout to `<program>/android/<code>/`.
/// Idempotent; a failed rename is logged and skipped, not fatal - the operator can move it by hand.
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
            // `android` means already relocated.
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

/// `Auth` + `RequireWrite` over `/api/v1/artifacts` and `/api/v1/apk` - every write here is
/// already `PUT`/`DELETE`, so the blanket `warehouse:write` grant is the right bar.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    let prefixes: [&'static str; 2] = [
        Box::leak(format!("{base_path}/api/v1/artifacts").into_boxed_str()),
        Box::leak(format!("{base_path}/api/v1/apk").into_boxed_str()),
    ];

    let mut app = app;
    for prefix in prefixes {
        app = wrap(
            app,
            OnPathPrefix::new(prefix, RequireWrite::new(jwt_config.clone())),
        );
        app = wrap(
            app,
            OnPathPrefix::new(prefix, Auth::new(jwt_config.clone())),
        );
    }
    app
}

pub fn register_routes() {
    ops::register_routes();
    ops::metadata::register_routes();
    ops::download::register_routes();
    alias::register_routes();
}
