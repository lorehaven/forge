use crate::docker_token::DockerTokenConfig;
use async_trait::async_trait;
use quench_http::prelude::http::{Method, StatusCode};
use quench_http::prelude::{Endpoint, Middleware, Request, Response};
use quench_starter::http::domain::error;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

static AUTH_FAILURES: LazyLock<Mutex<HashMap<String, Vec<Instant>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct WarehouseAuth {
    config: DockerTokenConfig,
    max_failures: usize,
    window: Duration,
}

impl WarehouseAuth {
    pub fn new(config: DockerTokenConfig) -> Self {
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

#[async_trait]
impl Middleware for WarehouseAuth {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        // Only protect /v2/*
        if !req.uri().path().starts_with("/v2/") {
            return next.call(req).await;
        }

        // Anonymous mode: bypass Bearer validation entirely.
        if !self.config.auth_enabled {
            return next.call(req).await;
        }

        if too_many_auth_failures(&req, self.max_failures, self.window) {
            return throttled(&self.config);
        }

        let Some(auth_header) = req.header("authorization") else {
            record_auth_failure(&req, self.window);
            return unauthorized(&self.config);
        };

        let Some(token) = auth_header.strip_prefix("Bearer ") else {
            record_auth_failure(&req, self.window);
            return unauthorized(&self.config);
        };

        let Ok(claims) = self.config.decode(token) else {
            record_auth_failure(&req, self.window);
            return unauthorized(&self.config);
        };

        if claims.service != self.config.service_name {
            record_auth_failure(&req, self.window);
            return unauthorized(&self.config);
        }

        clear_auth_failures(&req);

        if let Some((repository, action)) = repository_action(&req)
            && !scope_allows(&claims.scope, &repository, action)
        {
            return denied();
        }

        next.call(req).await
    }
}

fn throttled(config: &DockerTokenConfig) -> Response {
    let header = format!(
        "Bearer realm=\"{}\",service=\"{}\"",
        config.realm, config.service_name
    );
    error::response(
        StatusCode::TOO_MANY_REQUESTS,
        error::DENIED,
        "too many authentication attempts",
    )
    .header("www-authenticate", header)
}

fn denied() -> Response {
    error::response(
        StatusCode::FORBIDDEN,
        error::DENIED,
        "requested access to the resource is denied",
    )
}

pub fn repository_action(req: &Request) -> Option<(String, &'static str)> {
    let action = match *req.method() {
        Method::GET | Method::HEAD => "pull",
        Method::POST | Method::PATCH | Method::PUT | Method::DELETE => "push",
        _ => return None,
    };

    let path = req.uri().path();
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

pub fn scope_allows(scope: &str, repository: &str, action: &str) -> bool {
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

fn unauthorized(config: &DockerTokenConfig) -> Response {
    let header = format!(
        "Bearer realm=\"{}\",service=\"{}\"",
        config.realm, config.service_name
    );
    error::response(
        StatusCode::UNAUTHORIZED,
        error::UNAUTHORIZED,
        "authentication required",
    )
    .header("www-authenticate", header)
}

/// Rate-limiter bucket key. quench-http exposes no peer address, so this reads `x-forwarded-for`
/// only - degrades to one shared "unknown" bucket for a direct, unproxied connection.
fn client_key(req: &Request) -> String {
    req.header("x-forwarded-for")
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn too_many_auth_failures(req: &Request, max_failures: usize, window: Duration) -> bool {
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

pub fn record_auth_failure(req: &Request, window: Duration) {
    let key = client_key(req);
    let now = Instant::now();
    if let Ok(mut map) = AUTH_FAILURES.lock() {
        let entries = map.entry(key).or_default();
        entries.retain(|t| now.duration_since(*t) <= window);
        entries.push(now);
    }
}

pub fn clear_auth_failures(req: &Request) {
    let key = client_key(req);
    if let Ok(mut map) = AUTH_FAILURES.lock() {
        map.remove(&key);
    }
}
