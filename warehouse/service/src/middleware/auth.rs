use crate::shared::docker_error;
use crate::shared::jwt::{Claims, JwtConfig};
use actix_web::{
    Error,
    body::{EitherBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    http::header::{HeaderValue, WWW_AUTHENTICATE},
};
use futures_util::future::{LocalBoxFuture, Ready, ok};
use jsonwebtoken::{DecodingKey, Validation, decode};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

static AUTH_FAILURES: LazyLock<Mutex<HashMap<String, Vec<Instant>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct WarehouseAuth {
    config: JwtConfig,
    max_failures: usize,
    window: Duration,
}

impl WarehouseAuth {
    pub fn new(config: JwtConfig) -> Self {
        let max_failures = envmnt::get_or("MAX_AUTH_FAILURES_PER_MINUTE", "30")
            .parse()
            .unwrap_or(30);
        let window_secs = envmnt::get_or("AUTH_FAILURE_WINDOW_SECONDS", "60")
            .parse()
            .unwrap_or(60);

        Self {
            config,
            max_failures,
            window: Duration::from_secs(window_secs),
        }
    }
}

impl<S, B> Transform<S, ServiceRequest> for WarehouseAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = WarehouseAuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(WarehouseAuthMiddleware {
            service,
            config: self.config.clone(),
            max_failures: self.max_failures,
            window: self.window,
        })
    }
}

pub struct WarehouseAuthMiddleware<S> {
    service: S,
    config: JwtConfig,
    max_failures: usize,
    window: Duration,
}

impl<S, B> Service<ServiceRequest> for WarehouseAuthMiddleware<S>
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
        let path = req.path().to_string();

        // Only protect /v2/*
        if !path.starts_with("/v2/") {
            let fut = self.service.call(req);
            return Box::pin(async move {
                let res = fut.await?;
                Ok(res.map_into_left_body())
            });
        }

        if too_many_auth_failures(&req, self.max_failures, self.window) {
            return throttled(req, &self.config);
        }

        let auth_header = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok());

        if auth_header.is_none() {
            record_auth_failure(&req, self.window);
            return unauthorized(req, &self.config);
        }

        let token = auth_header.unwrap().strip_prefix("Bearer ");

        if token.is_none() {
            record_auth_failure(&req, self.window);
            return unauthorized(req, &self.config);
        }

        let validation = Validation::default();

        let decoded = decode::<Claims>(
            token.unwrap(),
            &DecodingKey::from_secret(self.config.jwt_secret.as_ref()),
            &validation,
        );

        if decoded.is_err() {
            record_auth_failure(&req, self.window);
            return unauthorized(req, &self.config);
        }

        let claims = decoded.unwrap().claims;

        if claims.service != self.config.service_name {
            record_auth_failure(&req, self.window);
            return unauthorized(req, &self.config);
        }

        clear_auth_failures(&req);

        if let Some((repository, action)) = repository_action(&req)
            && !scope_allows(&claims.scope, &repository, action)
        {
            return denied(req);
        }

        let fut = self.service.call(req);
        Box::pin(async move {
            let res = fut.await?;
            Ok(res.map_into_left_body())
        })
    }
}

fn throttled<B>(
    req: ServiceRequest,
    config: &JwtConfig,
) -> LocalBoxFuture<'static, Result<ServiceResponse<EitherBody<B>>, Error>>
where
    B: MessageBody + 'static,
{
    let header = format!(
        "Bearer realm=\"{}\",service=\"{}\"",
        config.realm, config.service_name
    );

    Box::pin(async move {
        let mut response = docker_error::response(
            actix_web::http::StatusCode::TOO_MANY_REQUESTS,
            docker_error::DENIED,
            "too many authentication attempts",
        );
        if let Ok(value) = HeaderValue::from_str(&header) {
            response.headers_mut().insert(WWW_AUTHENTICATE, value);
        }
        let response = response.map_into_right_body();
        Ok(req.into_response(response))
    })
}

fn denied<B>(
    req: ServiceRequest,
) -> LocalBoxFuture<'static, Result<ServiceResponse<EitherBody<B>>, Error>>
where
    B: MessageBody + 'static,
{
    Box::pin(async move {
        let response = docker_error::response(
            actix_web::http::StatusCode::FORBIDDEN,
            docker_error::DENIED,
            "requested access to the resource is denied",
        )
        .map_into_right_body();

        Ok(req.into_response(response))
    })
}

fn repository_action(req: &ServiceRequest) -> Option<(String, &'static str)> {
    let action = match *req.method() {
        actix_web::http::Method::GET | actix_web::http::Method::HEAD => "pull",
        actix_web::http::Method::POST
        | actix_web::http::Method::PATCH
        | actix_web::http::Method::PUT
        | actix_web::http::Method::DELETE => "push",
        _ => return None,
    };

    let path = req.path();
    let rest = path.strip_prefix("/v2/")?;

    if rest.is_empty() || rest.starts_with("_catalog") {
        return None;
    }

    for marker in ["/blobs/", "/manifests/", "/tags/list"] {
        if let Some((repo, _)) = rest.split_once(marker)
            && !repo.is_empty()
        {
            return Some((repo.to_string(), action));
        }
    }

    None
}

fn scope_allows(scope: &str, repository: &str, action: &str) -> bool {
    scope.split_whitespace().any(|entry| {
        let mut parts = entry.splitn(3, ':');
        let scope_type = parts.next().unwrap_or_default();
        let scope_repo = parts.next().unwrap_or_default();
        let scope_actions = parts.next().unwrap_or_default();

        if scope_type != "repository" {
            return false;
        }

        if scope_repo != repository && scope_repo != "*" {
            return false;
        }

        scope_actions
            .split(',')
            .any(|allowed| allowed == action || allowed == "*")
    })
}

fn unauthorized<B>(
    req: ServiceRequest,
    config: &JwtConfig,
) -> LocalBoxFuture<'static, Result<ServiceResponse<EitherBody<B>>, Error>>
where
    B: MessageBody + 'static,
{
    let header = format!(
        "Bearer realm=\"{}\",service=\"{}\"",
        config.realm, config.service_name
    );

    Box::pin(async move {
        let mut response = docker_error::response(
            actix_web::http::StatusCode::UNAUTHORIZED,
            docker_error::UNAUTHORIZED,
            "authentication required",
        );
        if let Ok(value) = HeaderValue::from_str(&header) {
            response.headers_mut().insert(WWW_AUTHENTICATE, value);
        }
        let response = response.map_into_right_body();

        Ok(req.into_response(response))
    })
}

fn client_key(req: &ServiceRequest) -> String {
    req.connection_info()
        .realip_remote_addr()
        .map(|s| s.to_string())
        .or_else(|| req.peer_addr().map(|a| a.ip().to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn too_many_auth_failures(req: &ServiceRequest, max_failures: usize, window: Duration) -> bool {
    let key = client_key(req);
    let now = Instant::now();
    let mut map = match AUTH_FAILURES.lock() {
        Ok(m) => m,
        Err(_) => return false,
    };

    let entries = map.entry(key).or_default();
    entries.retain(|t| now.duration_since(*t) <= window);
    entries.len() >= max_failures
}

fn record_auth_failure(req: &ServiceRequest, window: Duration) {
    let key = client_key(req);
    let now = Instant::now();
    if let Ok(mut map) = AUTH_FAILURES.lock() {
        let entries = map.entry(key).or_default();
        entries.retain(|t| now.duration_since(*t) <= window);
        entries.push(now);
    }
}

fn clear_auth_failures(req: &ServiceRequest) {
    let key = client_key(req);
    if let Ok(mut map) = AUTH_FAILURES.lock() {
        map.remove(&key);
    }
}
