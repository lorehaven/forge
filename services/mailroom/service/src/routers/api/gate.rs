//! Who may call what, and from where.
//!
//! `Auth` has already put the caller's claims on the request; the gate decides by the path and the method:
//!
//! * `/send` is for machine identities (the estate's services) and for an explicit grant of `mailroom:send`; a machine
//!   identity can do nothing else here;
//! * `/admin/...` needs `mailroom:admin`;
//! * anything else that only reads needs any grant of `mailroom`, and anything that changes something needs `write`.
//!
//! [`SameOrigin`] sits before it and refuses a state-changing request that carries a session cookie from another site.

use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::async_trait;
use quench_http::prelude::http::{Method, StatusCode};
use quench_http::prelude::{Endpoint, Middleware, Request, Response};

/// What a request asks of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Read,
    Write,
    Admin,
    Send,
}

/// What `method` on `path` needs, with the API under `api_prefix` (`/mailroom/api/v1`).
#[must_use]
pub fn need(method: &Method, path: &str, api_prefix: &str) -> Need {
    let under = |sub: &str| {
        let full = format!("{api_prefix}{sub}");
        path == full
            || path
                .strip_prefix(&full)
                .is_some_and(|rest| rest.starts_with('/'))
    };
    if under("/admin") {
        Need::Admin
    } else if under("/send") {
        Need::Send
    } else if matches!(*method, Method::GET | Method::HEAD) {
        Need::Read
    } else {
        Need::Write
    }
}

/// Whether the claims meet `need` for `service`.
///
/// Machine identities (the `service` role, a wildcard) may use the send endpoint and nothing else: a service's credential must
/// not open anybody's mailbox or the panel. And `send` is for them or for an explicit grant, not for a person's wildcard.
#[must_use]
pub fn meets(claims: &Claims, service: &str, need: Need) -> bool {
    let machine = claims.has_role("service");
    let explicit = |action: &str| {
        claims
            .permissions()
            .get(service)
            .is_some_and(|actions| actions.contains(action))
    };
    match need {
        Need::Send => machine || explicit("send"),
        _ if machine => false,
        Need::Admin => claims.can(service, "admin"),
        Need::Write => claims.can(service, "write") || claims.can(service, "admin"),
        Need::Read => ["read", "write", "admin"]
            .iter()
            .any(|a| claims.can(service, a)),
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
        let wanted = need(req.method(), req.uri().path(), &self.api_prefix);
        let allowed = req
            .extensions()
            .get::<Claims>()
            .is_some_and(|claims| meets(claims, &self.config.service_name, wanted));
        if allowed {
            next.call(req).await
        } else {
            quench_log::warn!(
                "mailroom: {} {} refused - needs {wanted:?}",
                req.method(),
                req.uri().path()
            );
            crate::error::Error::Forbidden.response()
        }
    }
}

/// Refuses a state-changing request that the browser says came from another site. A request with a bearer token has no cookie
/// to abuse and passes; so does one that no browser made (neither `Sec-Fetch-Site` nor `Origin`).
#[derive(Debug)]
pub struct SameOrigin {
    trusted: Vec<String>,
}

impl SameOrigin {
    #[must_use]
    pub const fn new(trusted: Vec<String>) -> Self {
        Self { trusted }
    }

    /// The decision, from the facts of a request.
    #[must_use]
    pub fn allows(
        &self,
        method: &Method,
        bearer: bool,
        fetch_site: Option<&str>,
        origin: Option<&str>,
        host: Option<&str>,
    ) -> bool {
        if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) || bearer {
            return true;
        }
        if let Some(site) = fetch_site {
            return matches!(site, "same-origin" | "none");
        }
        let Some(origin) = origin else {
            return true;
        };
        if self.trusted.iter().any(|t| t == origin) {
            return true;
        }
        let origin_host = origin.split_once("://").map_or(origin, |(_, rest)| rest);
        host.is_some_and(|h| h == origin_host)
    }
}

#[async_trait]
impl Middleware for SameOrigin {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let bearer = req
            .header("authorization")
            .is_some_and(|v| v.starts_with("Bearer "));
        let allowed = self.allows(
            req.method(),
            bearer,
            req.header("sec-fetch-site"),
            req.header("origin"),
            req.header("host"),
        );
        if allowed {
            next.call(req).await
        } else {
            quench_log::warn!(
                "mailroom: cross-site {} {} refused",
                req.method(),
                req.uri().path()
            );
            Response::new(StatusCode::FORBIDDEN)
        }
    }
}

/// What every API response carries: it is private, it is not kept, and it does not tell the next page where it came from.
#[derive(Debug)]
pub struct Private;

#[async_trait]
impl Middleware for Private {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let response = next.call(req).await;
        let response = if response.headers().contains_key("cache-control") {
            response
        } else {
            response.header("cache-control", "no-store")
        };
        if response.headers().contains_key("referrer-policy") {
            response
        } else {
            response.header("referrer-policy", "no-referrer")
        }
    }
}
