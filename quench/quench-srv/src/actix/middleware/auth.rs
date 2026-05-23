use crate::actix::domain::jwt::JwtConfig;
use actix_web::{
    Error, HttpMessage,
    body::{EitherBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};
use futures_util::future::{LocalBoxFuture, Ready, ok};
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
            service,
            config: self.config.clone(),
        })
    }
}

pub struct AuthMiddleware<S> {
    service: S,
    config: JwtConfig,
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
            let fut = self.service.call(req);
            return Box::pin(async move {
                let res = fut.await?;
                Ok(res.map_into_left_body())
            });
        }

        let mut token = None;

        // Check Authorization header
        if let Some(auth_header) = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            && let Some(bearer_token) = auth_header.strip_prefix("Bearer ")
        {
            token = Some(bearer_token.to_string());
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
                req.extensions_mut().insert(claims);
                let fut = self.service.call(req);
                Box::pin(async move {
                    let res = fut.await?;
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
