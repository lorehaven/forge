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

        // Check Authorization header
        if let Some(auth_header) = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
        {
            if let Some(bearer_token) = auth_header.strip_prefix("Bearer ") {
                token = Some(bearer_token.to_string());
            } else if let Some(basic_auth) = auth_header.strip_prefix("Basic ")
                && let Ok(decoded) =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, basic_auth)
            {
                let decoded_str = String::from_utf8_lossy(&decoded);
                if let Some((username, password)) = decoded_str.split_once(':') {
                    let user_db = req.app_data::<web::Data<crate::actix::domain::auth::UserDb>>();
                    if let Some(user_db) = user_db {
                        let user_db = user_db.clone();
                        let username = username.to_string();
                        let password = password.to_string();
                        let config = self.config.clone();
                        let service = self.service.clone();

                        return Box::pin(async move {
                            if let Some(user) = user_db.validate(&username, &password).await {
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
                                let res = actix_web::HttpResponse::Unauthorized()
                                    .finish()
                                    .map_into_right_body();
                                Ok(req.into_response(res))
                            }
                        });
                    }
                }
            }
        }

        // Check Cookie if no token in header
        if token.is_none() {
            let cookie_name = format!("{}_ui_session", self.config.service_name);
            if let Some(cookie) = req.cookie(&cookie_name) {
                token = Some(cookie.value().to_string());
            }
        }

        let Some(token_str) = token else {
            return Box::pin(async move {
                let res = actix_web::HttpResponse::Unauthorized()
                    .finish()
                    .map_into_right_body();
                Ok(req.into_response(res))
            });
        };

        match self.config.decode_claims(&token_str) {
            Ok(claims) => {
                if claims.service != self.config.service_name {
                    return Box::pin(async move {
                        let res = actix_web::HttpResponse::Unauthorized()
                            .finish()
                            .map_into_right_body();
                        Ok(req.into_response(res))
                    });
                }

                let session_db = req
                    .app_data::<web::Data<crate::actix::domain::session::SessionDb>>()
                    .cloned();
                let service = self.service.clone();
                Box::pin(async move {
                    if let Some(session_id) = claims.sid.as_deref() {
                        let active = match session_db {
                            Some(session_db) => session_db
                                .is_active(session_id, &claims.sub)
                                .await
                                .unwrap_or(false),
                            None => false,
                        };
                        if !active {
                            let res = actix_web::HttpResponse::Unauthorized()
                                .finish()
                                .map_into_right_body();
                            return Ok(req.into_response(res));
                        }
                    }

                    req.extensions_mut().insert(claims);
                    let res = service.call(req).await?;
                    Ok(res.map_into_left_body())
                })
            }
            Err(_) => Box::pin(async move {
                let res = actix_web::HttpResponse::Unauthorized()
                    .finish()
                    .map_into_right_body();
                Ok(req.into_response(res))
            }),
        }
    }
}
