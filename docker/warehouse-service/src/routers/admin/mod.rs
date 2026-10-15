use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_auth::http::middleware::require_write::RequireWrite;
use quench_http::prelude::{Endpoint, OnPathPrefix, wrap};
use std::sync::Arc;

pub mod crates;
pub mod docker;

pub fn register_routes() {
    crates::register_routes();
    docker::register_routes();
}

/// `Auth` + `RequireWrite` over `/admin`. Every admin route is a destructive `POST`, so the
/// blanket `warehouse:write` grant (or a wildcard role) is the bar - no read-only admin access.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/admin").into_boxed_str());

    let app = wrap(
        app,
        OnPathPrefix::new(prefix, RequireWrite::new(jwt_config.clone())),
    );
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}
