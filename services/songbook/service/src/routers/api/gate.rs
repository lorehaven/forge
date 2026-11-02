//! Who may call what. `Auth` has already put the caller's claims on the request; the gate decides by the path.
//!
//! Two kinds of caller share the service: a listener (any grant of `songbook`: `read`, or `write`) and the admin
//! (`songbook:write`, or a wildcard role). Everything under `/api/v1/admin` is the admin's; the rest of `/api/v1` is for
//! listeners, who may play and keep playlists but change nothing of the library.

use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::async_trait;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Endpoint, Middleware, Request, Response};

/// What a path asks of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Any grant of the service.
    Listener,
    /// `write`.
    Admin,
}

/// What `path` needs, with the API under `api_prefix` (`/songbook/api/v1`).
#[must_use]
pub fn need(path: &str, api_prefix: &str) -> Need {
    let admin = format!("{api_prefix}/admin");
    match path.strip_prefix(&admin) {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => Need::Admin,
        _ => Need::Listener,
    }
}

/// Whether these claims meet `need` for `service`.
#[must_use]
pub fn meets(claims: &Claims, service: &str, need: Need) -> bool {
    match need {
        Need::Admin => claims.can(service, "write"),
        Need::Listener => claims.can(service, "read") || claims.can(service, "write"),
    }
}

#[derive(Debug)]
pub struct Gate {
    config: JwtConfig,
    api_prefix: String,
}

impl Gate {
    #[must_use]
    pub fn new(config: JwtConfig, api_prefix: &str) -> Self {
        Self {
            config,
            api_prefix: api_prefix.to_string(),
        }
    }
}

#[async_trait]
impl Middleware for Gate {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        if !self.config.auth_enabled {
            return next.call(req).await;
        }
        let wanted = need(req.uri().path(), &self.api_prefix);
        let allowed = req
            .extensions()
            .get::<Claims>()
            .is_some_and(|claims| meets(claims, &self.config.service_name, wanted));
        if allowed {
            next.call(req).await
        } else {
            quench_log::warn!("songbook: {} refused - needs {wanted:?}", req.uri().path());
            Response::new(StatusCode::FORBIDDEN)
        }
    }
}
