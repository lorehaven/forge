use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::Endpoint;
use std::sync::Arc;

pub mod api;
pub mod ui;

/// No blanket `RequireWrite` here - see `api::wrap_auth` for why writes use
/// a per-resource `Claims::can` check instead; UI pages check their own session.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    api::wrap_auth(app, jwt_config, base_path)
}

pub fn register_routes() {
    api::register_routes();
    ui::register_routes();
}
