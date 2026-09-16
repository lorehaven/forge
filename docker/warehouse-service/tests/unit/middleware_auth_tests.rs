use crate::support;

use async_trait::async_trait;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::endpoint::Endpoint;
use quench_http::prelude::wrap;
use quench_http::request::Request;
use quench_http::response::Response;
use std::sync::Arc;
use std::time::Duration;
use warehouse_service::docker_token::{DockerClaims, DockerTokenConfig};
use warehouse_service::middleware::auth::{
    WarehouseAuth, clear_auth_failures, record_auth_failure, repository_action, scope_allows,
    too_many_auth_failures,
};

/// A container-less request - `repository_action`/`too_many_auth_failures`/
/// `record_auth_failure`/`clear_auth_failures` never touch the DI container,
/// so a dummy one is enough to satisfy `Request::new`.
fn plain_req(method: Method, path: &str) -> Request {
    let container = Arc::new(quench_http::di::Container::default());
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(bytes::Bytes::new()),
        container,
    )
}

/// Like `plain_req`, but with an `x-forwarded-for` header standing in for
/// the peer address actix's `TestRequest::peer_addr` used to set -
/// quench-http's `Request` carries no direct peer address at all (see
/// `middleware/auth.rs`'s own `client_key` doc comment), so the rate
/// limiter's per-client bucket key comes from this header instead.
fn req_from(method: Method, path: &str, peer: &str) -> Request {
    let container = Arc::new(quench_http::di::Container::default());
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", peer.parse().unwrap());
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(bytes::Bytes::new()),
        container,
    )
}

#[test]
fn scope_allows_exact_repository_match_with_the_requested_action() {
    assert!(scope_allows(
        "repository:my/repo:pull,push",
        "my/repo",
        "pull"
    ));
    assert!(scope_allows(
        "repository:my/repo:pull,push",
        "my/repo",
        "push"
    ));
}

#[test]
fn scope_allows_rejects_a_different_repository_or_action() {
    assert!(!scope_allows(
        "repository:my/repo:pull",
        "other/repo",
        "pull"
    ));
    assert!(!scope_allows("repository:my/repo:pull", "my/repo", "push"));
}

#[test]
fn scope_allows_wildcard_repository_and_action() {
    assert!(scope_allows("repository:*:pull", "anything/at-all", "pull"));
    assert!(scope_allows("repository:my/repo:*", "my/repo", "push"));
}

#[test]
fn scope_allows_matches_any_entry_in_a_multi_entry_scope() {
    let scope = "repository:other/repo:pull repository:my/repo:push";
    assert!(scope_allows(scope, "my/repo", "push"));
    assert!(!scope_allows(scope, "my/repo", "pull"));
}

#[test]
fn scope_allows_ignores_non_repository_scope_types() {
    assert!(!scope_allows("registry:catalog:*", "my/repo", "pull"));
}

#[test]
fn scope_allows_rejects_an_empty_scope() {
    assert!(!scope_allows("", "my/repo", "pull"));
}

#[test]
fn repository_action_maps_get_and_head_to_pull() {
    let req = plain_req(Method::GET, "/v2/my/repo/manifests/latest");
    assert_eq!(
        repository_action(&req),
        Some(("my/repo".to_string(), "pull"))
    );
}

#[test]
fn repository_action_maps_writes_to_push() {
    let req = plain_req(Method::POST, "/v2/my/repo/blobs/uploads/");
    assert_eq!(
        repository_action(&req),
        Some(("my/repo".to_string(), "push"))
    );
}

#[test]
fn repository_action_is_none_for_the_catalog_endpoint() {
    let req = plain_req(Method::GET, "/v2/_catalog");
    assert_eq!(repository_action(&req), None);
}

#[test]
fn repository_action_is_none_outside_v2() {
    let req = plain_req(Method::GET, "/api/v1/crates");
    assert_eq!(repository_action(&req), None);
}

#[test]
fn repository_action_is_none_without_a_recognized_marker() {
    let req = plain_req(Method::GET, "/v2/my/repo");
    assert_eq!(repository_action(&req), None);
}

#[test]
fn too_many_auth_failures_trips_after_the_configured_max_and_clear_resets_it() {
    let req = req_from(Method::GET, "/v2/my/repo/manifests/latest", "203.0.113.7");
    let window = Duration::from_secs(60);

    assert!(!too_many_auth_failures(&req, 3, window));
    record_auth_failure(&req, window);
    record_auth_failure(&req, window);
    assert!(!too_many_auth_failures(&req, 3, window));
    record_auth_failure(&req, window);
    assert!(too_many_auth_failures(&req, 3, window));

    clear_auth_failures(&req);
    assert!(!too_many_auth_failures(&req, 3, window));
}

#[test]
fn too_many_auth_failures_is_scoped_per_client() {
    let a = req_from(Method::GET, "/v2/my/repo/manifests/latest", "203.0.113.8");
    let b = req_from(Method::GET, "/v2/my/repo/manifests/latest", "203.0.113.9");
    let window = Duration::from_secs(60);

    record_auth_failure(&a, window);
    record_auth_failure(&a, window);
    assert!(too_many_auth_failures(&a, 2, window));
    assert!(!too_many_auth_failures(&b, 2, window));

    clear_auth_failures(&a);
}

/// `secret` is set through `DockerTokenConfig::init`, which reads
/// `DOCKER_TOKEN_SECRET` - the same fixed env var `docker_token_tests` uses,
/// hence the shared lock.
fn config(auth_enabled: bool) -> DockerTokenConfig {
    let _guard = support::secret_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    unsafe {
        std::env::set_var(
            "DOCKER_TOKEN_SECRET",
            "warehouse-auth-middleware-test-secret",
        )
    };
    let config = DockerTokenConfig::init(
        "warehouse".to_string(),
        "https://warehouse.test/token".to_string(),
        auth_enabled,
    );
    unsafe { std::env::remove_var("DOCKER_TOKEN_SECRET") };
    config
}

fn bearer(config: &DockerTokenConfig, scope: &str) -> String {
    bearer_for_service(config, &config.service_name, scope)
}

/// Like `bearer`, but with the claims' `service` set independently of
/// the signing config's own `service_name` - for testing a token that's
/// validly signed but minted for a different service.
fn bearer_for_service(config: &DockerTokenConfig, service: &str, scope: &str) -> String {
    let now = chrono::Utc::now();
    let claims = DockerClaims {
        sub: "alice".to_string(),
        service: service.to_string(),
        scope: scope.to_string(),
        iat: now.timestamp() as usize,
        exp: (now + chrono::Duration::minutes(5)).timestamp() as usize,
    };
    format!("Bearer {}", config.encode(&claims).expect("encode"))
}

/// The middleware's "next" - a fixed 200, since these tests care about what
/// `WarehouseAuth` does before (or instead of) calling it, not what a real
/// router would answer.
struct StubOk;

#[async_trait]
impl Endpoint for StubOk {
    async fn call(&self, _req: Request) -> Response {
        Response::new(StatusCode::OK)
    }
}

fn test_app(config: DockerTokenConfig) -> Arc<dyn Endpoint> {
    wrap(Arc::new(StubOk), WarehouseAuth::new(config))
}

fn req_with_headers(method: Method, path: &str, peer: &str, headers: &[(&str, &str)]) -> Request {
    let container = Arc::new(quench_http::di::Container::default());
    let mut header_map = HeaderMap::new();
    header_map.insert("x-forwarded-for", peer.parse().unwrap());
    for (name, value) in headers {
        header_map.insert(
            http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        header_map,
        quench_http::body::InboundBody::from_bytes(bytes::Bytes::new()),
        container,
    )
}

#[tokio::test]
async fn anonymous_mode_bypasses_bearer_validation_entirely() {
    let app = test_app(config(false));
    let req = req_with_headers(
        Method::GET,
        "/v2/my/repo/manifests/latest",
        "198.51.100.100",
        &[],
    );
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn requests_outside_v2_are_never_gated() {
    // Unlike the old actix test (which relied on a route pattern scoped to
    // `/v2/{tail:.*}` so a non-matching path 404ed at the router), this uses
    // a fixed-200 stub as "next" with no real router behind it - so "never
    // gated" here means the middleware passes it straight through to that
    // stub, observed as 200 rather than a 401/403 the middleware itself
    // would have produced had it tried to gate the request.
    let app = test_app(config(true));
    let req = req_with_headers(Method::GET, "/other", "198.51.100.101", &[]);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn missing_authorization_header_is_unauthorized() {
    let app = test_app(config(true));
    let req = req_with_headers(
        Method::GET,
        "/v2/my/repo/manifests/latest",
        "198.51.100.1",
        &[],
    );
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let (headers, _) = support::parts(resp).await;
    assert!(headers.contains_key("www-authenticate"));
}

#[tokio::test]
async fn non_bearer_authorization_header_is_unauthorized() {
    let app = test_app(config(true));
    let req = req_with_headers(
        Method::GET,
        "/v2/my/repo/manifests/latest",
        "198.51.100.2",
        &[("authorization", "Basic dXNlcjpwYXNz")],
    );
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_token_for_a_different_service_is_unauthorized() {
    let config = config(true);
    let app = test_app(config.clone());
    let token = bearer_for_service(&config, "someone-else", "repository:my/repo:pull");
    let req = req_with_headers(
        Method::GET,
        "/v2/my/repo/manifests/latest",
        "198.51.100.3",
        &[("authorization", &token)],
    );
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_valid_token_without_matching_scope_is_forbidden() {
    let config = config(true);
    let app = test_app(config.clone());
    let token = bearer(&config, "repository:other/repo:pull");
    let req = req_with_headers(
        Method::GET,
        "/v2/my/repo/manifests/latest",
        "198.51.100.4",
        &[("authorization", &token)],
    );
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_valid_token_with_matching_scope_is_let_through() {
    let config = config(true);
    let app = test_app(config.clone());
    let token = bearer(&config, "repository:my/repo:pull");
    let req = req_with_headers(
        Method::GET,
        "/v2/my/repo/manifests/latest",
        "198.51.100.5",
        &[("authorization", &token)],
    );
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn repeated_failures_from_the_same_client_eventually_get_throttled() {
    // `MAX_AUTH_FAILURES_PER_MINUTE` defaults to 30 in `WarehouseAuth::new`
    // when unset, which would make this loop impractically long, so pin it
    // low for this test. It's a fixed env var name `new()` reads once at
    // construction time, so no cross-test lock is needed here - the value
    // only matters for the instant `test_app` builds this test's own config.
    unsafe { std::env::set_var("MAX_AUTH_FAILURES_PER_MINUTE", "2") };
    let app = test_app(config(true));
    // Safe to clear immediately after: `WarehouseAuth::new` (called inside
    // `test_app`, above) reads the env var once at construction time, not
    // per-request.
    unsafe { std::env::remove_var("MAX_AUTH_FAILURES_PER_MINUTE") };

    let peer = "198.51.100.6";
    for _ in 0..2 {
        let req = req_with_headers(Method::GET, "/v2/my/repo/manifests/latest", peer, &[]);
        let resp = app.call(req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    let req = req_with_headers(Method::GET, "/v2/my/repo/manifests/latest", peer, &[]);
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}
