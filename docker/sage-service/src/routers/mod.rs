use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_auth::http::middleware::require_write::RequireWrite;
use quench_http::prelude::{Endpoint, OnPathPrefix, wrap};
use std::sync::Arc;

pub mod chat;
pub mod files;
pub mod ui;

/// `Auth` must wrap `RequireWrite` (outermost-first composition, unlike
/// actix), and prefixes need `base_path` since `OnPathPrefix` sees the raw un-mounted path.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    let prefixes: [&'static str; 5] = [
        Box::leak(format!("{base_path}/api/v1/files").into_boxed_str()),
        Box::leak(format!("{base_path}/api/v1/chat").into_boxed_str()),
        Box::leak(format!("{base_path}/ui/chat").into_boxed_str()),
        Box::leak(format!("{base_path}/ui/projects").into_boxed_str()),
        Box::leak(format!("{base_path}/ui/files").into_boxed_str()),
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
    chat::register_routes();
    files::register_routes();
    ui::register_routes();
}
