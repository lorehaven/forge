//! Gantry's HTTP API; everything sits behind the realm's `Auth`.

use crate::domain::GantryError;
use async_trait::async_trait;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::http::middleware::auth::Auth;
use quench_http::prelude::{
    Endpoint, FromRequest, HttpError, OnPathPrefix, Request, http::StatusCode, wrap,
};
pub use quench_starter::http::domain::api_error::{ApiError, json_error};

pub mod authz;
pub mod deployments;
pub mod info;
pub mod operations;
pub mod resources;
pub mod targets;

/// The verified identity, if any - `Auth` puts it in the request extensions.
pub struct OptionalClaims(pub Option<Claims>);

#[async_trait]
impl FromRequest for OptionalClaims {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(OptionalClaims(req.extensions().get::<Claims>().cloned()))
    }
}

/// Who is making this request, for the `requested_by` column: the token's subject (a person, or an OAuth
/// client id for a machine), or `dev` when auth is switched off.
pub fn actor(claims: Option<&Claims>) -> String {
    claims
        .map(|claims| claims.sub.clone())
        .unwrap_or_else(|| "dev".to_string())
}

/// `Auth` over all of `/api/v1` (incl. its 404 fallback, so unmapped paths 401 not 404); what a caller may
/// *do* is checked per route against `authz`.
pub fn wrap_auth(
    app: std::sync::Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> std::sync::Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/api/v1").into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}

pub fn register_routes() {
    info::register_routes();
    deployments::register_routes();
    operations::register_routes();
    resources::register_routes();
    targets::register_routes();
}

impl From<GantryError> for ApiError {
    fn from(error: GantryError) -> Self {
        if error.is_unique_violation() {
            return ApiError::new(
                StatusCode::CONFLICT,
                "a record with that identity already exists",
            );
        }

        let status = match &error {
            // Not the caller's fault: deployment has no real database.
            GantryError::NotPostgres => StatusCode::SERVICE_UNAVAILABLE,
            GantryError::Sql(_) | GantryError::Plan(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        ApiError::new(status, error.to_string())
    }
}
