use crate::routers::crates_storage_root;
use async_trait::async_trait;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_auth::http::middleware::require_write::RequireWrite;
use quench_http::prelude::http::Method;
use quench_http::prelude::{Endpoint, Middleware, Request, Response, wrap};
use std::path::PathBuf;
use std::sync::Arc;

pub mod index;
pub mod ops;
pub mod owners;
pub mod search;

/// On-disk path for a `.crate` tarball: `<root>/<n>/<version>/<n>-<version>.crate`.
#[must_use]
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
#[must_use]
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
#[must_use]
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
#[must_use]
pub fn validate_crate_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    name.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Validates a semver-ish version string: non-empty, ≤64 chars, safe characters only.
#[must_use]
pub fn validate_version(version: &str) -> bool {
    if version.is_empty() || version.len() > 64 {
        return false;
    }
    version
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
}

/// Whether index/download/search/owner-listing need a realm identity too.
///
/// Off by default so
/// existing Cargo clients (which only send a token when the index says `auth-required`) keep
/// resolving dependencies; every write is authenticated either way.
#[must_use]
pub fn reads_require_auth() -> bool {
    envmnt::get_or("CRATES_REQUIRE_AUTH_FOR_READS", "false")
        .parse()
        .unwrap_or(false)
}

/// Authenticates the crates registry: the JSON API (`/api/v1/crates`) and the sparse index
/// (`/index`). Writes (publish, yank/unyank, owner changes) always go through `Auth` +
/// `RequireWrite`, i.e. the blanket `warehouse:write` grant; reads do only when
/// `reads_require_auth`. Cargo sends its configured token verbatim, so a client stores it as
/// `Bearer <jwt>`.
struct CratesAuth {
    prefixes: [String; 2],
    reads_require_auth: bool,
    guarded: Arc<dyn Endpoint>,
}

#[async_trait]
impl Middleware for CratesAuth {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let path = req.uri().path();
        let in_scope = self.prefixes.iter().any(|prefix| {
            path.strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        });
        let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);

        if in_scope && (!safe || self.reads_require_auth) {
            self.guarded.call(req).await
        } else {
            next.call(req).await
        }
    }
}

pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
    reads_require_auth: bool,
) -> Arc<dyn Endpoint> {
    // `Auth` outermost so `RequireWrite` can see its claims.
    let guarded = wrap(app.clone(), RequireWrite::new(jwt_config.clone()));
    let guarded = wrap(guarded, Auth::new(jwt_config));

    wrap(
        app,
        CratesAuth {
            prefixes: [
                format!("{base_path}/api/v1/crates"),
                format!("{base_path}/index"),
            ],
            reads_require_auth,
            guarded,
        },
    )
}

pub fn register_routes() {
    search::register_routes();
    ops::register_routes();
    owners::register_routes();
    index::register_routes();
}
