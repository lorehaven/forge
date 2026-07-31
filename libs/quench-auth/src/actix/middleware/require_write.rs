//! Enforces the write half of a permission.
//!
//! `Auth` establishes who is asking and, through the audience narrowing
//! gatehouse does at token issue, whether they may reach this service at all -
//! a token for someone with no grant on this service simply is not valid here.
//! What audience narrowing cannot do is tell a `read` grant from a `write` one:
//! only the service that owns the routes knows which of them mutate anything.
//!
//! `RequireWrite` closes that gap with one rule: a request whose method is not
//! GET/HEAD/OPTIONS needs the `"write"` action (or a wildcard role) on the
//! token. Mount it once and the rule covers the whole scope; where a route
//! breaks the method-shape assumption - a POST-shaped read, a GET-shaped write,
//! or a write that needs a *different*, more specific action than the generic
//! `"write"` (switchboard's `"launch"`/`"stop"`/`"delete-model"`, say) - guard
//! that one route directly with [`crate::actix::domain::jwt::Claims::can`]
//! instead of trying to make this middleware smarter than the routes under it.
//! A service whose writes are all like that has no business behind this
//! middleware at all; mount per-route checks instead.
//!
//! **Mounting order matters.** This middleware trusts the [`Claims`] already
//! sitting in the request's extensions, put there by [`Auth`](super::auth::Auth).
//! Actix runs the *last*-registered `.wrap()` *first*, so `Auth` has to be the
//! outer layer - the last `.wrap()` call - for its claims to exist by the time
//! this one runs:
//!
//! ```ignore
//! web::scope("/api/v1/things")
//!     .wrap(RequireWrite::new(config.clone()))
//!     .wrap(Auth::new(config))
//! ```
//!
//! Mounted the other way round, every write would 403: there would be no
//! `Claims` in extensions yet for this middleware to read, and an absent claim
//! is treated as no permission rather than as "ask `Auth` first". That is the
//! safe failure direction for a mounting mistake to fail in.

use crate::actix::domain::jwt::{Claims, JwtConfig};
use actix_web::{
    Error, HttpMessage,
    body::{EitherBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    http::Method,
};
use futures_util::future::{LocalBoxFuture, Ready, ok};
use std::rc::Rc;
use std::task::{Context, Poll};

pub struct RequireWrite {
    config: JwtConfig,
}

impl RequireWrite {
    pub fn new(config: JwtConfig) -> Self {
        Self { config }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RequireWrite
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = RequireWriteMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(RequireWriteMiddleware {
            service: Rc::new(service),
            config: self.config.clone(),
        })
    }
}

pub struct RequireWriteMiddleware<S> {
    service: Rc<S>,
    config: JwtConfig,
}

impl<S, B> Service<ServiceRequest> for RequireWriteMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // The realm-wide dev switch: with auth off there is no verified identity
        // to check a permission against, and `Auth` itself is a no-op under the
        // same flag - this has to match, or turning auth off would turn write
        // access off instead of leaving it unchecked like everything else.
        let bypass = !self.config.auth_enabled || is_safe(req.method());

        if bypass {
            let service = Rc::clone(&self.service);
            return Box::pin(async move {
                let res = service.call(req).await?;
                Ok(res.map_into_left_body())
            });
        }

        let allowed = req
            .extensions()
            .get::<Claims>()
            .is_some_and(|claims| claims.can(&self.config.service_name, "write"));

        if allowed {
            let service = Rc::clone(&self.service);
            Box::pin(async move {
                let res = service.call(req).await?;
                Ok(res.map_into_left_body())
            })
        } else {
            tracing::warn!(
                "RequireWrite: {} {} refused - no write permission on {}",
                req.method(),
                req.path(),
                self.config.service_name
            );
            Box::pin(async move {
                let res = actix_web::HttpResponse::Forbidden()
                    .finish()
                    .map_into_right_body();
                Ok(req.into_response(res))
            })
        }
    }
}

/// Methods that never need `write`, whatever the route behind them does.
///
/// `TRACE` and `CONNECT` are deliberately absent: nothing in this estate serves
/// them, and treating an unrecognised method as safe would be the wrong default
/// for a middleware whose entire job is "deny unless told otherwise".
fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

#[cfg(test)]
mod tests {
    use super::is_safe;
    use actix_web::http::Method;

    #[test]
    fn only_get_head_and_options_are_safe() {
        assert!(is_safe(&Method::GET));
        assert!(is_safe(&Method::HEAD));
        assert!(is_safe(&Method::OPTIONS));
        assert!(!is_safe(&Method::POST));
        assert!(!is_safe(&Method::PUT));
        assert!(!is_safe(&Method::PATCH));
        assert!(!is_safe(&Method::DELETE));
    }
}
