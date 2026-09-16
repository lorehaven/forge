//! That every API route is reachable, and that the right ones need a token.
//!
//! This exists because one of them was not reachable. `POST /repos/{id}/runs`
//! was declared beside the `/repos` scope rather than inside it, and actix picks
//! the first scope whose path matches without falling through to the next - so
//! the request entered `/repos`, found no match, and 404ed. Every handler was
//! correct and fully unit-tested; the URL simply did not reach them.
//!
//! The same trap applies to the webhook endpoint, which has to sit outside the
//! auth middleware while living under the same `/api/v1` prefix as everything
//! that sits inside it.
//!
//! quench-http's router carries a version of the same risk under a different
//! name: an ambiguous pair of patterns (a literal path segment and a `{id}`
//! wildcard at the same position) resolves by *registration order*
//! (link-order-dependent via `inventory`), not by a real static-first trie
//! the way actix-router matched - so two routes shaped like that can silently
//! swap which one wins across a rebuild. This file's job is unchanged: prove
//! every declared route actually resolves to its handler, not just that the
//! handler itself is correct in isolation.
//!
//! Route resolution is checked with auth *off*. With it on, the middleware
//! answers 401 before matching ever happens, so "not a 404" would be true of
//! every URL including nonsense - the test would pass against an API that had
//! no routes at all.

use bytes::Bytes;
use conveyor_service::config::ConveyorConfig;
use conveyor_service::providers::Providers;
use conveyor_service::routers::api;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::di::ContainerBuilder;
use quench_http::request::Request;
use std::sync::OnceLock;
use tokio::sync::Mutex;

/// `JwtConfig::init` reads the environment, which the whole binary shares, so
/// the two modes take turns rather than racing.
fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

enum Body {
    None,
    Json(serde_json::Value),
}

async fn status_with_auth(auth: bool, method: Method, path: &str, body: Body) -> StatusCode {
    let _guard = lock().lock().await;

    // Every other test in this binary that builds a `JwtConfig` via
    // `for_tests()` (which reads `SERVICE_AUTH_ENABLED` at construction, see
    // `quench_auth::domain::jwt::JwtConfig::from_parts`) expects auth
    // to default off. Leaving `true` set here after this function returns
    // would leak into whichever test the binary happens to run next.
    let previous = std::env::var("SERVICE_AUTH_ENABLED").ok();
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", if auth { "true" } else { "false" }) };

    let db = quench_db::prelude::Db::connect("")
        .await
        .expect("in-memory database");
    let jwt_config = JwtConfig::for_tests();

    api::register_routes();
    let container = ContainerBuilder::new()
        .provide(db)
        .provide(jwt_config.clone())
        .provide(ConveyorConfig::default())
        .provide_arc(std::sync::Arc::new(Providers::from_env()))
        .build()
        .await
        .unwrap();
    let container = std::sync::Arc::new(container);

    let app = quench_starter::http::discover_and_mount("/");
    let app = api::wrap_auth(app, jwt_config, "");

    let mut headers = HeaderMap::new();
    let body_bytes = match &body {
        Body::None => Bytes::new(),
        Body::Json(value) => {
            headers.insert("content-type", "application/json".parse().unwrap());
            Bytes::from(serde_json::to_vec(value).unwrap())
        }
    };
    let request = Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(body_bytes),
        container,
    );

    let status = app.call(request).await.status();

    match previous {
        Some(value) => unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", value) },
        None => unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") },
    }

    status
}

/// Every route behind the realm's auth, and a request shaped to reach it.
fn authenticated_routes() -> Vec<(&'static str, Method, &'static str, Body)> {
    vec![
        ("GET /repos", Method::GET, "/api/v1/repos", Body::None),
        (
            "POST /repos",
            Method::POST,
            "/api/v1/repos",
            Body::Json(
                serde_json::json!({ "owner": "o", "name": "n", "clone_url": "file:///tmp/x" }),
            ),
        ),
        (
            "GET /repos/{id}",
            Method::GET,
            "/api/v1/repos/abc",
            Body::None,
        ),
        (
            "PATCH /repos/{id}",
            Method::PATCH,
            "/api/v1/repos/abc",
            Body::Json(serde_json::json!({ "enabled": true })),
        ),
        (
            "POST /repos/{id}/enabled",
            Method::POST,
            "/api/v1/repos/abc/enabled",
            Body::Json(serde_json::json!({ "enabled": false })),
        ),
        (
            "DELETE /repos/{id}",
            Method::DELETE,
            "/api/v1/repos/abc",
            Body::None,
        ),
        (
            "POST /repos/{id}/runs",
            Method::POST,
            "/api/v1/repos/abc/runs",
            Body::Json(serde_json::json!({})),
        ),
        ("GET /runs", Method::GET, "/api/v1/runs", Body::None),
        (
            "GET /runs/{id}",
            Method::GET,
            "/api/v1/runs/abc",
            Body::None,
        ),
        (
            "POST /runs/{id}/cancel",
            Method::POST,
            "/api/v1/runs/abc/cancel",
            Body::None,
        ),
        (
            "GET /jobs/{id}/logs",
            Method::GET,
            "/api/v1/jobs/abc/logs",
            Body::None,
        ),
    ]
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_declared_route_resolves_to_a_handler() {
    for (name, method, path, body) in authenticated_routes() {
        assert_ne!(
            status_with_auth(false, method, path, body).await,
            StatusCode::NOT_FOUND,
            "{name} did not resolve"
        );
    }
}

#[tokio::test]
async fn triggering_a_run_resolves() {
    // The one that was broken, on its own so a failure names it.
    assert_ne!(
        status_with_auth(
            false,
            Method::POST,
            "/api/v1/repos/some-id/runs",
            Body::Json(serde_json::json!({}))
        )
        .await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn an_undeclared_route_is_a_404() {
    // The check that keeps the tests above from being tautologies.
    assert_eq!(
        status_with_auth(false, Method::GET, "/api/v1/nonsense", Body::None).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn an_unknown_provider_is_a_404() {
    assert_eq!(
        status_with_auth(
            false,
            Method::POST,
            "/api/v1/webhooks/gitlab",
            Body::Json(serde_json::json!({}))
        )
        .await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_queue_refuses_an_in_memory_database_rather_than_using_it() {
    // A queue on top of one would look like it worked and lose every queued run
    // on restart.
    assert_eq!(
        status_with_auth(false, Method::GET, "/api/v1/runs", Body::None).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_api_needs_a_token() {
    for (name, method, path, body) in authenticated_routes() {
        assert_eq!(
            status_with_auth(true, method, path, body).await,
            StatusCode::UNAUTHORIZED,
            "{name} should have required a token"
        );
    }
}

#[tokio::test]
async fn webhooks_are_reachable_without_a_token() {
    // A provider has no realm token; its delivery is authenticated by its
    // signature instead.
    let status = status_with_auth(
        true,
        Method::POST,
        "/api/v1/webhooks/github",
        Body::Json(serde_json::json!({})),
    )
    .await;

    assert_ne!(status, StatusCode::NOT_FOUND, "the route should resolve");
    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "webhooks must not sit behind the realm's auth"
    );
}
