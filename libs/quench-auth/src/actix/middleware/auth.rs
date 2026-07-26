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

        let mut token = None;

        // Log all headers for debugging
        tracing::debug!("AuthMiddleware: Received request to {}", req.path());
        for (name, value) in req.headers().iter() {
            tracing::debug!("AuthMiddleware: Header {} = {:?}", name, value);
        }

        // Check Authorization header
        if let Some(auth_header) = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
        {
            tracing::debug!(
                "AuthMiddleware: Found Authorization header: {}",
                auth_header
            );

            if let Some(bearer_token) = auth_header.strip_prefix("Bearer ") {
                token = Some(bearer_token.to_string());
            } else if let Some(basic_auth) = auth_header.strip_prefix("Basic ")
                && let Ok(decoded) =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, basic_auth)
            {
                let decoded_str = String::from_utf8_lossy(&decoded);
                if let Some((username, password)) = decoded_str.split_once(':') {
                    tracing::debug!(
                        "AuthMiddleware: Basic auth decoded - username: '{}', password length: {}",
                        username,
                        password.len()
                    );
                    let user_db = req
                        .app_data::<web::Data<std::sync::Arc<crate::actix::domain::auth::UserDb>>>(
                        );
                    if let Some(user_db) = user_db {
                        tracing::debug!("AuthMiddleware: UserDb found in app_data");
                        let user_db = user_db.clone();
                        let username = username.to_string();
                        let password = password.to_string();
                        let config = self.config.clone();
                        let service = self.service.clone();

                        return Box::pin(async move {
                            tracing::debug!("AuthMiddleware: Validating user: {}", username);
                            if let Some(user) = user_db.validate(&username, &password).await {
                                tracing::debug!(
                                    "AuthMiddleware: User {} validated successfully",
                                    username
                                );
                                let roles = user
                                    .get_roles()
                                    .iter()
                                    .map(|r| format!("{:?}", r).to_lowercase())
                                    .collect::<Vec<_>>()
                                    .join(" ");

                                let claims = crate::actix::domain::jwt::Claims::new(
                                    user.username.clone(),
                                    config.service_name.clone(),
                                    roles,
                                    None,
                                    3600,
                                );
                                req.extensions_mut().insert(claims);
                                let res = service.call(req).await?;
                                Ok(res.map_into_left_body())
                            } else {
                                tracing::warn!(
                                    "AuthMiddleware: Basic auth validation failed for user: {}",
                                    username
                                );
                                let res = actix_web::HttpResponse::Unauthorized()
                                    .finish()
                                    .map_into_right_body();
                                Ok(req.into_response(res))
                            }
                        });
                    } else {
                        tracing::error!("AuthMiddleware: UserDb not found in app_data!");
                    }
                }
            }
        }

        // Check Cookie if no token in header
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

        match self.config.decode_claims(&token_str) {
            Ok(claims) => {
                if !claims.allows(&self.config.service_name) {
                    tracing::warn!(
                        "AuthMiddleware: Token audience mismatch. Expected {}, got {:?}",
                        self.config.service_name,
                        claims.aud
                    );
                    return Box::pin(async move {
                        let res = unauthorized(&req).map_into_right_body();
                        Ok(req.into_response(res))
                    });
                }

                let session_db = req
                    .app_data::<web::Data<std::sync::Arc<crate::actix::domain::session::SessionDb>>>()
                    .cloned();
                let service = self.service.clone();
                Box::pin(async move {
                    if let Some(session_id) = claims.sid.as_deref() {
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
            Err(e) => {
                tracing::warn!("AuthMiddleware: Failed to decode claims: {:?}", e);
                Box::pin(async move {
                    let res = unauthorized(&req).map_into_right_body();
                    Ok(req.into_response(res))
                })
            }
        }
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
