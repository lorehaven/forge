//! Conveyor's HTTP API - everything sits behind the realm's `Auth` middleware.

use crate::scheduler::QueueError;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::http::middleware::auth::Auth;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_core::async_trait;
use quench_http::prelude::{
    Endpoint, FromRequest, HttpError, Middleware, OnPathPrefix, Request, Response, wrap,
};
pub use quench_starter::http::domain::api_error::{ApiError, json_error};
use std::sync::Arc;

pub mod authz;
pub mod credentials;
pub mod overview;
pub mod projects;
pub mod repos;
pub mod runs;
pub mod secrets;
pub mod stream;
pub mod subscriptions;
pub mod webhooks;

/// The verified identity behind this request, if any - `Auth` puts it in the request's extensions.
#[derive(Debug)]
pub struct OptionalClaims(pub Option<Claims>);

#[async_trait]
impl FromRequest for OptionalClaims {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(req.extensions().get::<Claims>().cloned()))
    }
}

/// Who is making this request, for `registered_by`/`owner` columns. A `FromRequest` extractor since
/// quench-http has no extractor that hands a handler the raw request itself.
#[derive(Debug)]
pub struct Actor(pub String);

#[async_trait]
impl FromRequest for Actor {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        // Missing `JwtConfig` only happens if the app is misconfigured.
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self("dev".to_string()));
        };
        let username = get_user_from_req(req, &config)
            .await
            .map_or_else(|| "dev".to_string(), |claims| claims.sub);
        Ok(Self(username))
    }
}

/// `Auth` on every `/api/v1/*` path except `/api/v1/webhooks/*` (signature-authenticated instead) -
/// gates the whole prefix so an unmapped path 401s rather than leaking a 404.
struct ApiAuth {
    auth: Auth,
    webhooks_prefix: &'static str,
}

#[async_trait]
impl Middleware for ApiAuth {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        if req.uri().path().starts_with(self.webhooks_prefix) {
            return next.call(req).await;
        }
        self.auth.handle(req, next).await
    }
}

/// `RequireWrite` is deliberately not mounted here - resource-scoped grants need `authz::can_on_project`
/// per route instead; `Auth` alone gates every route with a verified identity.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    let api_prefix: &'static str = Box::leak(format!("{base_path}/api/v1").into_boxed_str());
    let webhooks_prefix: &'static str =
        Box::leak(format!("{base_path}/api/v1/webhooks").into_boxed_str());

    wrap(
        app,
        OnPathPrefix::new(
            api_prefix,
            ApiAuth {
                auth: Auth::new(jwt_config),
                webhooks_prefix,
            },
        ),
    )
}

pub fn register_routes() {
    credentials::register_routes();
    overview::register_routes();
    projects::register_routes();
    repos::register_routes();
    runs::register_routes();
    secrets::register_routes();
    stream::register_routes();
    subscriptions::register_routes();
    webhooks::register_routes();
}

impl From<QueueError> for ApiError {
    fn from(error: QueueError) -> Self {
        if let QueueError::Sql(database) = &error
            && database.code() == Some("23503")
        {
            // `registered_by` means the requesting account isn't in the realm's `users` table at all.
            let message = if matches!(
                database
                    .server()
                    .and_then(|server| server.constraint.as_deref()),
                Some("repos_registered_by_fkey" | "subscriptions_username_fkey")
            ) {
                "the account making this request is not in the realm; \
                 sign in through gatehouse, which owns the estate's users"
            } else {
                "the id given for a related record (project, repository, ...) \
                 does not exist"
            };
            return Self::new(quench_http::http::StatusCode::BAD_REQUEST, message);
        }

        if let QueueError::Sql(database) = &error
            && database.code() == Some("23505")
        {
            return Self::new(
                quench_http::http::StatusCode::CONFLICT,
                "a record with that identity already exists",
            );
        }

        let status = match &error {
            // Not the caller's fault - deployment is configured without a real database.
            QueueError::NotPostgres => quench_http::http::StatusCode::SERVICE_UNAVAILABLE,
            QueueError::UnknownRepo(_) | QueueError::UnknownRun(_) => {
                quench_http::http::StatusCode::NOT_FOUND
            }
            QueueError::BadRow(_) | QueueError::Sql(_) => {
                quench_http::http::StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        Self::new(status, error.to_string())
    }
}
