//! Workbench's HTTP API.
//!
//! Everything here sits behind the realm's `Auth` middleware, so a handler can
//! assume there is a signed-in identity and only has to decide what to do.

use crate::domain::WorkbenchError;
use actix_web::http::StatusCode;
use actix_web::{HttpMessage, HttpRequest, web};
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::actix::routers::ui::get_user_from_req;
use quench_auth::prelude::{Claims, JwtConfig};
pub use quench_starter::prelude::{ApiError, json_error};

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
    let Some(config) = request.app_data::<web::Data<JwtConfig>>() else {
        return "dev".to_string();
    };
    get_user_from_req(request, config)
        .await
        .map(|claims| claims.sub)
        .unwrap_or_else(|| "dev".to_string())
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
            // Not the caller's fault and not something a retry fixes: the
            // deployment is configured without a real database.
            WorkbenchError::NotPostgres => StatusCode::SERVICE_UNAVAILABLE,
            WorkbenchError::Sql(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        ApiError::new(status, error.to_string())
    }
}
