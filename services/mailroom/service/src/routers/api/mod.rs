//! Mailroom's HTTP API; everything under `/api/v1` sits behind the realm's `Auth` and, after it, the gate.

use crate::error::Error;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::http::middleware::auth::Auth;
use quench_core::async_trait;
use quench_http::prelude::{
    Endpoint, FromRequest, HttpError, OnPathPrefix, Request, Response, http::StatusCode, wrap,
};
use quench_serde::Serialize;

/// What a JSON request body may weigh (the biggest is a message of a megabyte of text).
pub const JSON_LIMIT: usize = 2 * 1024 * 1024;
/// What the services' send request may weigh: a megabyte of text, two of HTML, headers.
pub const SEND_LIMIT: usize = 4 * 1024 * 1024;

pub mod admin;
pub mod gate;
pub mod messages;
pub mod send;

/// The prefix `Auth` and the gate guard. `BASE_PATH=/` gives `/api/v1`, not `//api/v1`.
#[must_use]
pub fn protected_prefix(base_path: &str) -> String {
    format!("{}/api/v1", base_path.trim_end_matches('/'))
}

/// `Auth` over all of `/api/v1` (its 404 fallback included, so unmapped paths answer 401), then the gate.
pub fn wrap_auth(
    app: std::sync::Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
    trusted_origins: Vec<String>,
) -> std::sync::Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(protected_prefix(base_path).into_boxed_str());
    // The last wrap runs first: Auth, then the same-origin check, then the gate (which reads the claims Auth put on the request).
    let app = wrap(
        app,
        OnPathPrefix::new(prefix, gate::Gate::new(jwt_config.clone(), prefix)),
    );
    let app = wrap(
        app,
        OnPathPrefix::new(prefix, gate::SameOrigin::new(trusted_origins)),
    );
    let app = wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)));
    wrap(app, OnPathPrefix::new(prefix, gate::Private))
}

pub fn register_routes() {
    admin::register_routes();
    messages::register_routes();
    send::register_routes();
}

/// Who is asking, as Mailroom needs to know.
#[derive(Debug, Clone)]
pub struct Caller {
    /// The user name (`sub`); `dev` when authentication is off.
    pub name: String,
    pub admin: bool,
    pub write: bool,
}

#[async_trait]
impl FromRequest for Caller {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let config = req
            .container()
            .get::<JwtConfig>()
            .map_err(|e| HttpError::status(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if !config.auth_enabled {
            return Ok(Self {
                name: "dev".into(),
                admin: true,
                write: true,
            });
        }
        let claims = req
            .extensions()
            .get::<Claims>()
            .cloned()
            .ok_or_else(|| HttpError::status(StatusCode::UNAUTHORIZED, "sign in first"))?;
        let service = &config.service_name;
        Ok(Self {
            admin: claims.can(service, "admin"),
            write: claims.can(service, "write") || claims.can(service, "admin"),
            name: claims.sub.to_ascii_lowercase(),
        })
    }
}

/// `200` with `value` as JSON, or the error as the API sends it.
#[must_use]
pub fn reply<T: Serialize>(result: crate::error::Result<T>) -> Response {
    match result {
        Ok(value) => Response::json(StatusCode::OK, &value)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(e) => e.response(),
    }
}

/// `201` with `value` as JSON.
#[must_use]
pub fn created<T: Serialize>(result: crate::error::Result<T>) -> Response {
    match result {
        Ok(value) => Response::json(StatusCode::CREATED, &value)
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(e) => e.response(),
    }
}

/// `204`, or the error.
#[must_use]
pub fn done(result: crate::error::Result<()>) -> Response {
    match result {
        Ok(()) => Response::new(StatusCode::NO_CONTENT),
        Err(e) => e.response(),
    }
}

/// An error from a handler that has nothing else to say.
#[must_use]
pub fn error(status: StatusCode, text: &str) -> Response {
    let _ = text;
    match status {
        StatusCode::NOT_FOUND => Error::NotFound.response(),
        StatusCode::FORBIDDEN => Error::Forbidden.response(),
        _ => Error::Invalid(text.to_string()).response(),
    }
}
