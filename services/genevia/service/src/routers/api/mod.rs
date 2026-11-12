//! Genevia's HTTP API; everything under `/api/v1` sits behind the realm's `Auth`, and each route checks the caller's
//! grants for the tree it names (see [`authz`]).

use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_http::prelude::{Endpoint, OnPathPrefix, Response, http::StatusCode, wrap};
use quench_serde::Serialize;
pub use quench_starter::http::domain::api_error::json_error;

pub mod authz;
pub mod trees;

/// `Auth` over all of `/api/v1` (incl. its 404 fallback, so unmapped paths 401 not 404).
pub fn wrap_auth(
    app: std::sync::Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> std::sync::Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(protected_prefix(base_path).into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}

/// The path prefix `Auth` guards. `BASE_PATH=/` is "/" after normalising: without the trim the prefix would be `//api/v1`
/// and protect nothing.
#[must_use]
pub fn protected_prefix(base_path: &str) -> String {
    format!("{}/api/v1", base_path.trim_end_matches('/'))
}

pub fn ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    trees::register_routes();
}
