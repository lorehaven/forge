//! The API: `GET /v1/apps`, `GET /v1/apps/{package}` and `GET /v1/categories`, under the service's base path and
//! behind the realm's login (any valid identity; nothing here is per user).

use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_http::prelude::{Endpoint, OnPathPrefix, wrap};

pub mod apps;

/// `Auth` over everything under `{base_path}/v1`: scoped so the kubelet's health probes stay unauthenticated.
pub fn wrap_auth(
    app: std::sync::Arc<dyn Endpoint>,
    config: JwtConfig,
    base_path: &str,
) -> std::sync::Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/v1").into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(config)))
}

pub fn register_routes() {
    apps::register_routes();
}
