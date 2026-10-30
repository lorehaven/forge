use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_auth::http::middleware::require_write::RequireWrite;
use quench_http::prelude::{Endpoint, OnPathPrefix, wrap};
use std::sync::Arc;

pub mod chat;
pub mod conversations;
pub mod files;
pub mod models;
pub mod turns;
pub mod ui;

/// `Auth` must wrap `RequireWrite` (outermost-first composition, unlike
/// actix), and prefixes need `base_path` since `OnPathPrefix` sees the raw un-mounted path.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: &JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    // Everything the API and the browser client call; the pages themselves check the session on their own.
    let prefixes: Vec<&'static str> = [
        "/api/v1/files",
        "/api/v1/chat",
        "/api/v1/models",
        "/api/v1/conversation-id",
        "/api/v1/conversations",
        "/api/v1/projects",
        "/api/v1/turns",
        "/api/v1/messages",
    ]
    .iter()
    .map(|path| &*Box::leak(format!("{base_path}{path}").into_boxed_str()))
    .collect();

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
    conversations::register_routes();
    files::register_routes();
    models::register_routes();
    turns::register_routes();
    ui::register_routes();
}
