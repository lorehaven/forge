//! Authenticated, owner-scoped JSON API. No browser routes or local login UI.

use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::http::middleware::auth::Auth;
use quench_core::async_trait;
use quench_http::prelude::{Endpoint, FromRequest, HttpError, OnPathPrefix, Request, wrap};

pub mod api_error;
pub mod authz;
pub mod labels;
pub mod projects;
pub mod tasks;

/// The verified identity, if any - `Auth` puts it in the request extensions.
#[derive(Debug)]
pub struct OptionalClaims(pub Option<Claims>);

#[async_trait]
impl FromRequest for OptionalClaims {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(req.extensions().get::<Claims>().cloned()))
    }
}

/// Who is making this request - the owner every row is scoped to.
#[must_use]
pub fn actor(claims: Option<&Claims>) -> String {
    claims.map_or_else(|| "dev".to_string(), |claims| claims.sub.clone())
}

/// `Auth` over all of `/api/v1` (incl. its 404 fallback, so unmapped paths 401
/// not 404); `daybook:read` / `daybook:write` are checked per route via `authz`.
pub fn wrap_auth(
    app: std::sync::Arc<dyn Endpoint>,
    config: JwtConfig,
    base_path: &str,
) -> std::sync::Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/api/v1").into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(config)))
}

pub fn register_routes() {
    labels::register_routes();
    projects::register_routes();
    tasks::register_routes();
}
