//! Conveyor's HTTP API.
//!
//! Everything here sits behind the realm's `Auth` middleware, so a handler can
//! assume there is a signed-in identity and only has to decide what to do.

use crate::scheduler::QueueError;
use actix_web::http::StatusCode;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, web};
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::prelude::{Claims, JwtConfig, realm};
use serde_json::json;

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
    // The borrow is scoped deliberately. `extensions()` hands out a `Ref` that
    // otherwise lives to the end of the statement, and `session_subject` reads
    // a cookie - which makes actix take `extensions_mut()` to cache the parsed
    // cookie jar. Both at once panics with "RefCell already borrowed".
    let from_token = {
        let extensions = request.extensions();
        extensions.get::<Claims>().map(|claims| claims.sub.clone())
    };

    match from_token {
        Some(sub) => sub,
        None => session_subject(request)
            .await
            // `SERVICE_AUTH_ENABLED=false` lets the middleware through without
            // an identity, which is the only way to get here. `dev` is what
            // the rest of the estate calls that account.
            .unwrap_or_else(|| "dev".to_string()),
    }
}

/// The realm cookie, for a call that came from a browser rather than from a
/// client sending a bearer token.
async fn session_subject(request: &HttpRequest) -> Option<String> {
    let cookie = request.cookie(&realm::session_cookie_name())?;
    let config = request.app_data::<web::Data<JwtConfig>>()?;
    config
        .decode_claims(cookie.value())
        .await
        .ok()
        .map(|c| c.sub)
}

pub fn json_error(status: StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(json!({ "error": message }))
}

/// A queue error, translated into something a caller can act on.
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn into_response(self) -> HttpResponse {
        if self.status.is_server_error() {
            tracing::error!("api error: {}", self.message);
        }
        json_error(self.status, &self.message)
    }
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
                    .to_string()
            } else {
                "the id given for a related record (project, repository, ...) \
                 does not exist"
                    .to_string()
            };
            return Self {
                status: StatusCode::BAD_REQUEST,
                message,
            };
        }

        if let QueueError::Sql(sqlx::Error::Database(database)) = &error
            && database.code().as_deref() == Some("23505")
        {
            return Self {
                status: StatusCode::CONFLICT,
                message: "a record with that identity already exists".to_string(),
            };
        }

        let status = match &error {
            // Not the caller's fault and not something a retry fixes: the
            // deployment is configured without a real database.
            QueueError::NotPostgres => StatusCode::SERVICE_UNAVAILABLE,
            QueueError::UnknownRepo(_) | QueueError::UnknownRun(_) => StatusCode::NOT_FOUND,
            QueueError::BadRow(_) | QueueError::Sql(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        Self {
            status,
            message: error.to_string(),
        }
    }
}
