//! Workbench's HTTP API; everything sits behind the realm's `Auth`.

use crate::domain::WorkbenchError;
use async_trait::async_trait;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::http::middleware::auth::Auth;
use quench_http::prelude::{
    Endpoint, FromRequest, HttpError, OnPathPrefix, Request, http::StatusCode, wrap,
};
pub use quench_starter::http::domain::api_error::{ApiError, json_error};

pub mod authz;
pub mod comments;
pub mod issue_links;
pub mod issues;
pub mod labels;
pub mod projects;

/// The verified identity, if any - `Auth` puts it in the request extensions.
pub struct OptionalClaims(pub Option<Claims>);

#[async_trait]
impl FromRequest for OptionalClaims {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(OptionalClaims(req.extensions().get::<Claims>().cloned()))
    }
}

/// Who is making this request, for `reporter` and `author` columns.
pub fn actor(claims: Option<&Claims>) -> String {
    claims
        .map(|claims| claims.sub.clone())
        .unwrap_or_else(|| "dev".to_string())
}

/// `Auth` over all of `/api/v1` (incl. its 404 fallback, so unmapped
/// paths 401 not 404); writes are guarded per-route via `authz` instead.
pub fn wrap_auth(
    app: std::sync::Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> std::sync::Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/api/v1").into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}

pub fn register_routes() {
    comments::register_routes();
    issue_links::register_routes();
    issues::register_routes();
    labels::register_routes();
    projects::register_routes();
}

impl From<WorkbenchError> for ApiError {
    fn from(error: WorkbenchError) -> Self {
        if error.is_foreign_key_violation() {
            return ApiError::new(
                StatusCode::BAD_REQUEST,
                "the id given for a related record (project, issue, ...) does not exist",
            );
        }

        if error.is_unique_violation() {
            return ApiError::new(
                StatusCode::CONFLICT,
                "a record with that identity already exists",
            );
        }

        let status = match &error {
            // Not the caller's fault: deployment has no real database.
            WorkbenchError::NotPostgres => StatusCode::SERVICE_UNAVAILABLE,
            WorkbenchError::Sql(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        ApiError::new(status, error.to_string())
    }
}
