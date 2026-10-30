//! Gauge's HTTP API; everything sits behind the realm's `Auth`.

use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_http::prelude::{Endpoint, OnPathPrefix, wrap};
pub use quench_starter::http::domain::api_error::{ApiError, json_error};

pub mod hosts;

/// `Auth` over all of `/api/v1` (incl. its 404 fallback, so unmapped paths 401 not 404).
pub fn wrap_auth(
    app: std::sync::Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> std::sync::Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/api/v1").into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}

pub fn register_routes() {
    hosts::register_routes();
}
