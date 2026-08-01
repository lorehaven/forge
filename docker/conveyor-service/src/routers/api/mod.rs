//! Conveyor's HTTP API.
//!
//! Everything here sits behind the realm's `Auth` middleware, so a handler can
//! assume there is a signed-in identity and only has to decide what to do.

use crate::scheduler::QueueError;
use actix_web::http::StatusCode;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, web};
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::actix::middleware::require_write::RequireWrite;
use quench_auth::prelude::{Claims, JwtConfig, realm};
use serde_json::json;

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
/// Every other scope has a clean method shape - registering, enabling,
/// deleting a repo; writing or deleting a secret; starting or cancelling a
/// run - all POST/PUT/DELETE, with every GET a genuine read. `RequireWrite`
/// needs no route-level exceptions here, so it stacks on `Auth` the same way
/// every other service in the estate does it.
///
/// Registration order is load-bearing. Actix picks the first service whose path
/// matches and does not fall through to the next, so the concrete webhook route
/// comes first, then `/repos`, and the catch-all `scope("")` that holds the run
/// routes comes last - anything after it would be unreachable.
pub fn scope(jwt_config: JwtConfig) -> actix_web::Scope {
    web::scope("/api/v1")
        .service(webhooks::receive)
        .service(
            secrets::scope()
                .wrap(RequireWrite::new(jwt_config.clone()))
                .wrap(Auth::new(jwt_config.clone())),
        )
        .service(
            repos::scope()
                .wrap(RequireWrite::new(jwt_config.clone()))
                .wrap(Auth::new(jwt_config.clone())),
        )
        .service(
            runs::scope()
                .wrap(RequireWrite::new(jwt_config.clone()))
                .wrap(Auth::new(jwt_config)),
        )
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
    config.decode_claims(cookie.value()).await.ok().map(|c| c.sub)
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
        // A foreign key violation here is almost always one thing: the account
        // making the request is not in the realm's `users` table. Raw, that
        // reads as "repos_registered_by_fkey", which tells the caller nothing
        // about what to do.
        if let QueueError::Sql(sqlx::Error::Database(database)) = &error
            && database.code().as_deref() == Some("23503")
        {
            return Self {
                status: StatusCode::BAD_REQUEST,
                message: "the account making this request is not in the realm; \
                          sign in through gatehouse, which owns the estate's users"
                    .to_string(),
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
