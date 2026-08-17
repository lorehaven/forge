//! Workbench's HTTP API.
//!
//! Everything here sits behind the realm's `Auth` middleware, so a handler can
//! assume there is a signed-in identity and only has to decide what to do.

use crate::domain::WorkbenchError;
use actix_web::http::StatusCode;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, web};
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::prelude::{Claims, JwtConfig, realm};
use serde_json::json;

pub mod authz;
pub mod comments;
pub mod issue_links;
pub mod issues;
pub mod labels;
pub mod projects;

/// The API, with auth applied where it belongs. `RequireWrite` is not mounted
/// here, for the same reason conveyor's isn't: once a project can be scoped
/// to a specific grant (`workbench:project:<id>:write`), the generic "holds
/// `write` on `workbench`" check is both too strict and not specific enough -
/// guard each route directly with `authz::can_on_project` instead. `Auth`
/// alone still gates every route: a verified identity with `workbench` as an
/// audience.
pub fn scope(jwt_config: JwtConfig) -> actix_web::Scope {
    web::scope("/api/v1")
        .service(projects::scope().wrap(Auth::new(jwt_config.clone())))
        .service(issues::scope().wrap(Auth::new(jwt_config.clone())))
        .service(labels::scope().wrap(Auth::new(jwt_config.clone())))
        .service(comments::scope().wrap(Auth::new(jwt_config.clone())))
        .service(issue_links::scope().wrap(Auth::new(jwt_config.clone())))
        // Catches whatever the scopes above don't own. Registration order is
        // load-bearing (actix picks the first scope whose prefix matches), so
        // this comes last. It has no routes of its own, so anything reaching
        // it 404s - but `Auth` still runs first, so an unauthenticated caller
        // gets 401 rather than being able to map the API by probing unknown
        // paths under it.
        .service(web::scope("").wrap(Auth::new(jwt_config)))
}

/// The verified identity behind this request, if any. `Auth` puts it in the
/// request's extensions; every route that needs a resource-scoped check reads
/// it from here rather than re-verifying anything.
pub fn claims(request: &HttpRequest) -> Option<Claims> {
    request.extensions().get::<Claims>().cloned()
}

/// Who is making this request, for `reporter` and `author` columns.
pub async fn actor(request: &HttpRequest) -> String {
    // The borrow is scoped deliberately - see conveyor's `actor` for why:
    // `extensions()` hands out a `Ref` that otherwise lives to the end of the
    // statement, and `session_subject` takes `extensions_mut()` to cache the
    // parsed cookie jar. Both at once panics with "RefCell already borrowed".
    let from_token = {
        let extensions = request.extensions();
        extensions.get::<Claims>().map(|claims| claims.sub.clone())
    };

    match from_token {
        Some(sub) => sub,
        None => session_subject(request)
            .await
            .unwrap_or_else(|| "dev".to_string()),
    }
}

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

impl From<WorkbenchError> for ApiError {
    fn from(error: WorkbenchError) -> Self {
        if error.is_foreign_key_violation() {
            return Self {
                status: StatusCode::BAD_REQUEST,
                message: "the id given for a related record (project, issue, ...) \
                          does not exist"
                    .to_string(),
            };
        }

        if error.is_unique_violation() {
            return Self {
                status: StatusCode::CONFLICT,
                message: "a record with that identity already exists".to_string(),
            };
        }

        let status = match &error {
            // Not the caller's fault and not something a retry fixes: the
            // deployment is configured without a real database.
            WorkbenchError::NotPostgres => StatusCode::SERVICE_UNAVAILABLE,
            WorkbenchError::Sql(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        Self {
            status,
            message: error.to_string(),
        }
    }
}
