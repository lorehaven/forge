//! Conveyor's HTTP API.
//!
//! Everything here sits behind the realm's `Auth` middleware, so a handler can
//! assume there is a signed-in identity and only has to decide what to do.

use crate::scheduler::QueueError;
use actix_web::http::StatusCode;
use actix_web::{HttpMessage, HttpRequest, web};
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::actix::routers::ui::get_user_from_req;
use quench_auth::prelude::{Claims, JwtConfig};
pub use quench_starter::prelude::{ApiError, json_error};

pub mod authz;
pub mod credentials;
pub mod projects;
pub mod repos;
pub mod runs;
pub mod secrets;
pub mod stream;
pub mod webhooks;

/// The API, with auth applied where it belongs.
///
/// Webhooks are deliberately outside it: a provider has no realm token, and its
/// delivery is authenticated by its signature instead.
///
/// `RequireWrite` is *not* mounted here, unlike every scope-that-only-needs-
/// coarse-write in the estate: once a repo or project can be scoped to a
/// specific grant (`conveyor:project:<id>:write`), the generic "holds `write`
/// on `conveyor`" check it does is both too strict (a resource-scoped grant
/// with no blanket `write` would 403 before reaching the route) and not
/// specific enough (it can't tell "write repo A" from "write repo B"). This is
/// exactly the situation `RequireWrite`'s own doc comment calls out - guard the
/// route directly with `Claims::can` (here, via `authz::can_on_project`)
/// instead of stretching the middleware. `Auth` alone still gates every route:
/// a verified identity with `conveyor` as an audience.
///
/// Registration order is load-bearing. Actix picks the first service whose path
/// matches and does not fall through to the next, so the concrete webhook route
/// comes first, then `/repos` and `/projects`, and the catch-all `scope("")`
/// that holds the run routes comes last - anything after it would be
/// unreachable.
pub fn scope(jwt_config: JwtConfig) -> actix_web::Scope {
    web::scope("/api/v1")
        .service(webhooks::receive)
        .service(secrets::scope().wrap(Auth::new(jwt_config.clone())))
        .service(projects::scope().wrap(Auth::new(jwt_config.clone())))
        .service(repos::scope().wrap(Auth::new(jwt_config.clone())))
        .service(runs::scope().wrap(Auth::new(jwt_config)))
}

/// The verified identity behind this request, if any. `Auth` puts it in the
/// request's extensions; every route that needs a resource-scoped check reads
/// it from here rather than re-verifying anything.
pub fn claims(request: &HttpRequest) -> Option<Claims> {
    request.extensions().get::<Claims>().cloned()
}

/// Who is making this request, for the `registered_by` and `owner` columns.
///
/// The `Auth` middleware has already established that there is a valid token;
/// this only reads the name out of it. `unknown` is unreachable behind that
/// middleware and is not a fallback anyone should rely on.
pub async fn actor(request: &HttpRequest) -> String {
    // No `JwtConfig` in app_data only happens if the app is misconfigured.
    let Some(config) = request.app_data::<web::Data<JwtConfig>>() else {
        return "dev".to_string();
    };
    get_user_from_req(request, config)
        .await
        .map(|claims| claims.sub)
        .unwrap_or_else(|| "dev".to_string())
}

impl From<QueueError> for ApiError {
    fn from(error: QueueError) -> Self {
        if let QueueError::Sql(sqlx::Error::Database(database)) = &error
            && database.code().as_deref() == Some("23503")
        {
            // `registered_by` is the one case that reads as something other
            // than "the id you named does not exist" - it means the account
            // making the request is not in the realm's `users` table at all.
            let message = if database.constraint() == Some("repos_registered_by_fkey") {
                "the account making this request is not in the realm; \
                 sign in through gatehouse, which owns the estate's users"
            } else {
                "the id given for a related record (project, repository, ...) \
                 does not exist"
            };
            return ApiError::new(StatusCode::BAD_REQUEST, message);
        }

        if let QueueError::Sql(sqlx::Error::Database(database)) = &error
            && database.code().as_deref() == Some("23505")
        {
            return ApiError::new(
                StatusCode::CONFLICT,
                "a record with that identity already exists",
            );
        }

        let status = match &error {
            // Not the caller's fault and not something a retry fixes: the
            // deployment is configured without a real database.
            QueueError::NotPostgres => StatusCode::SERVICE_UNAVAILABLE,
            QueueError::UnknownRepo(_) | QueueError::UnknownRun(_) => StatusCode::NOT_FOUND,
            QueueError::BadRow(_) | QueueError::Sql(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        ApiError::new(status, error.to_string())
    }
}
