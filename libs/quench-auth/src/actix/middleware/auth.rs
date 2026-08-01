use crate::actix::domain::jwt::JwtConfig;
use actix_web::{
    Error, HttpMessage,
    body::{EitherBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    web,
};
use futures_util::future::{LocalBoxFuture, Ready, ok};
use std::rc::Rc;
use std::task::{Context, Poll};

pub struct Auth {
    config: JwtConfig,
}

impl Auth {
    pub fn new(config: JwtConfig) -> Self {
        Self { config }
    }
}

impl<S, B> Transform<S, ServiceRequest> for Auth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = AuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(AuthMiddleware {
            service: Rc::new(service),
            config: self.config.clone(),
        })
    }
}

pub struct AuthMiddleware<S> {
    service: Rc<S>,
    config: self::JwtConfig,
}

impl<S, B> Service<ServiceRequest> for AuthMiddleware<S>
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
        if !self.config.auth_enabled {
            let service = self.service.clone();
            return Box::pin(async move {
                let res = service.call(req).await?;
                Ok(res.map_into_left_body())
            });
        }

        // Every caller - human or machine - arrives as a bearer token or the
        // realm session cookie now. There is no more password-bearing branch
        // here: a machine identity gets its token from gatehouse's
        // client_credentials grant the same way a browser gets one from the
        // authorization-code flow, so this middleware only ever verifies.
        let mut token = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::to_string);

        if token.is_none()
            && let Some(cookie) = req.cookie(&crate::actix::domain::realm::session_cookie_name())
        {
            token = Some(cookie.value().to_string());
        }

        let Some(token_str) = token else {
            tracing::warn!("AuthMiddleware: No token found in header or cookie");
            return Box::pin(async move {
                let res = unauthorized(&req).map_into_right_body();
                Ok(req.into_response(res))
            });
        };

        let config = self.config.clone();
        let service = self.service.clone();
        Box::pin(async move {
            let claims = match config.decode_claims(&token_str).await {
                Ok(claims) => claims,
                Err(e) => {
                    tracing::warn!("AuthMiddleware: Failed to decode claims: {:?}", e);
                    let res = unauthorized(&req).map_into_right_body();
                    return Ok(req.into_response(res));
                }
            };

            if !claims.allows(&config.service_name) {
                tracing::warn!(
                    "AuthMiddleware: Token audience mismatch. Expected {}, got {:?}",
                    config.service_name,
                    claims.aud
                );
                let res = unauthorized(&req).map_into_right_body();
                return Ok(req.into_response(res));
            }

            if let Some(session_id) = claims.sid.as_deref() {
                let session_db = req
                    .app_data::<web::Data<std::sync::Arc<crate::actix::domain::session::SessionDb>>>()
                    .cloned();
                let active = match session_db {
                    Some(session_db) => session_db
                        .is_active(session_id, &claims.sub)
                        .await
                        .unwrap_or(false),
                    None => {
                        tracing::warn!("AuthMiddleware: SessionDb not found in app_data");
                        false
                    }
                };
                if !active {
                    tracing::warn!(
                        "AuthMiddleware: Session {} is not active for user {}",
                        session_id,
                        claims.sub
                    );
                    let res = unauthorized(&req).map_into_right_body();
                    return Ok(req.into_response(res));
                }
            }

            req.extensions_mut().insert(claims);
            let res = service.call(req).await?;
            Ok(res.map_into_left_body())
        })
    }
}

/// 401 for API callers; a redirect to the gatehouse login for browsers, so an
/// expired session lands on the realm login page instead of a blank error.
fn unauthorized(req: &ServiceRequest) -> actix_web::HttpResponse {
    if wants_html(req)
        && let Some(login_url) =
            crate::actix::domain::realm::gatehouse_login_url(Some(&req.uri().to_string()))
    {
        return actix_web::HttpResponse::Found()
            .append_header(("Location", login_url))
            .finish();
    }

    actix_web::HttpResponse::Unauthorized()
        .append_header(("WWW-Authenticate", "Bearer"))
        .finish()
}

fn wants_html(req: &ServiceRequest) -> bool {
    req.headers()
        .get("Accept")
        .and_then(|accept| accept.to_str().ok())
        .is_some_and(|accept| accept.contains("text/html"))
}
