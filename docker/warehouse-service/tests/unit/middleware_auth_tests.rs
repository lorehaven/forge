use crate::support;

use actix_web::test::TestRequest;
use std::time::Duration;
use warehouse_service::docker_token::{DockerClaims, DockerTokenConfig};
use warehouse_service::middleware::auth::{
    WarehouseAuth, clear_auth_failures, record_auth_failure, repository_action, scope_allows,
    too_many_auth_failures,
};

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
    let req = TestRequest::get()
        .uri("/v2/my/repo/manifests/latest")
        .to_srv_request();
    assert_eq!(
        repository_action(&req),
        Some(("my/repo".to_string(), "pull"))
    );
}

#[test]
fn repository_action_maps_writes_to_push() {
    let req = TestRequest::post()
        .uri("/v2/my/repo/blobs/uploads/")
        .to_srv_request();
    assert_eq!(
        repository_action(&req),
        Some(("my/repo".to_string(), "push"))
    );
}

#[test]
fn repository_action_is_none_for_the_catalog_endpoint() {
    let req = TestRequest::get().uri("/v2/_catalog").to_srv_request();
    assert_eq!(repository_action(&req), None);
}

#[test]
fn repository_action_is_none_outside_v2() {
    let req = TestRequest::get().uri("/api/v1/crates").to_srv_request();
    assert_eq!(repository_action(&req), None);
}

#[test]
fn repository_action_is_none_without_a_recognized_marker() {
    let req = TestRequest::get().uri("/v2/my/repo").to_srv_request();
    assert_eq!(repository_action(&req), None);
}

#[test]
fn too_many_auth_failures_trips_after_the_configured_max_and_clear_resets_it() {
    let req = TestRequest::default()
        .peer_addr("203.0.113.7:12345".parse().unwrap())
        .to_srv_request();
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
    let a = TestRequest::default()
        .peer_addr("203.0.113.8:1".parse().unwrap())
        .to_srv_request();
    let b = TestRequest::default()
        .peer_addr("203.0.113.9:1".parse().unwrap())
        .to_srv_request();
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

/// A macro, not a function: `test::init_service`'s return type is opaque
/// and can't be named without pulling in `actix-http` as a direct
/// dev-dependency just to spell `actix_http::Request`, so this expands
/// inline at each call site instead of trying to name it.
macro_rules! test_app {
    ($config:expr) => {{
        use actix_web::{App, HttpResponse, web};
        actix_web::test::init_service(App::new().wrap(WarehouseAuth::new($config)).route(
            "/v2/{tail:.*}",
            web::route().to(|| async { HttpResponse::Ok().finish() }),
        ))
        .await
    }};
}

#[actix_web::test]
async fn anonymous_mode_bypasses_bearer_validation_entirely() {
    let app = test_app!(config(false));
    let req = TestRequest::get()
        .uri("/v2/my/repo/manifests/latest")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn requests_outside_v2_are_never_gated() {
    // No route matches `/other` at all, so a non-404 here would mean the
    // middleware itself, not routing, decided the outcome; a 404 proves
    // the middleware passed the request straight through.
    let app = test_app!(config(true));
    let req = TestRequest::get().uri("/other").to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn missing_authorization_header_is_unauthorized() {
    let app = test_app!(config(true));
    let req = TestRequest::get()
        .uri("/v2/my/repo/manifests/latest")
        .peer_addr("198.51.100.1:1".parse().unwrap())
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    assert!(
        resp.headers()
            .contains_key(actix_web::http::header::WWW_AUTHENTICATE)
    );
}

#[actix_web::test]
async fn non_bearer_authorization_header_is_unauthorized() {
    let app = test_app!(config(true));
    let req = TestRequest::get()
        .uri("/v2/my/repo/manifests/latest")
        .peer_addr("198.51.100.2:1".parse().unwrap())
        .insert_header(("Authorization", "Basic dXNlcjpwYXNz"))
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn a_token_for_a_different_service_is_unauthorized() {
    let config = config(true);
    let app = test_app!(config.clone());
    let req = TestRequest::get()
        .uri("/v2/my/repo/manifests/latest")
        .peer_addr("198.51.100.3:1".parse().unwrap())
        .insert_header((
            "Authorization",
            bearer_for_service(&config, "someone-else", "repository:my/repo:pull"),
        ))
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn a_valid_token_without_matching_scope_is_forbidden() {
    let config = config(true);
    let app = test_app!(config.clone());
    let req = TestRequest::get()
        .uri("/v2/my/repo/manifests/latest")
        .peer_addr("198.51.100.4:1".parse().unwrap())
        .insert_header((
            "Authorization",
            bearer(&config, "repository:other/repo:pull"),
        ))
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
}

#[actix_web::test]
async fn a_valid_token_with_matching_scope_is_let_through() {
    let config = config(true);
    let app = test_app!(config.clone());
    let req = TestRequest::get()
        .uri("/v2/my/repo/manifests/latest")
        .peer_addr("198.51.100.5:1".parse().unwrap())
        .insert_header(("Authorization", bearer(&config, "repository:my/repo:pull")))
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn repeated_failures_from_the_same_client_eventually_get_throttled() {
    // `MAX_AUTH_FAILURES_PER_MINUTE` defaults to 30 in `WarehouseAuth::new`
    // when unset, which would make this loop impractically long, so pin it
    // low for this test. It's a fixed env var name `new()` reads once at
    // construction time, so no cross-test lock is needed here - the value
    // only matters for the instant `app()` builds this test's own config.
    unsafe { std::env::set_var("MAX_AUTH_FAILURES_PER_MINUTE", "2") };
    let app = test_app!(config(true));
    // Safe to clear immediately after: `WarehouseAuth::new` (called by
    // `.wrap()` inside `app()`, above) reads the env var once at
    // construction time, not per-request.
    unsafe { std::env::remove_var("MAX_AUTH_FAILURES_PER_MINUTE") };

    let peer = "198.51.100.6:1".parse().unwrap();
    for _ in 0..2 {
        let req = TestRequest::get()
            .uri("/v2/my/repo/manifests/latest")
            .peer_addr(peer)
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    let req = TestRequest::get()
        .uri("/v2/my/repo/manifests/latest")
        .peer_addr(peer)
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::TOO_MANY_REQUESTS
    );
}
