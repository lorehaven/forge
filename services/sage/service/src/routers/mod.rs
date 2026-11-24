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

/// An unexpected failure: the detail goes to the log, the caller gets a key the UI translates.
///
/// A value the database cannot store (a NUL byte, text that is not UTF-8, out of range, too long for its column or index:
/// SQLSTATE class 22, or 54000) is the caller's fault and answers 400.
pub(crate) fn failure(detail: impl std::fmt::Display) -> quench_http::response::Response {
    use quench_http::prelude::http::StatusCode;
    let detail = detail.to_string();
    if detail.contains("SQLSTATE 22") || detail.contains("SQLSTATE 54000") {
        return quench_http::response::Response::text(
            StatusCode::BAD_REQUEST,
            "api_error_invalid_value",
        );
    }
    quench_log::error!("Internal error: {detail}");
    quench_http::response::Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
}
