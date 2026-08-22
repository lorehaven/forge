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
//! Route resolution is checked with auth *off*. With it on, the middleware
//! answers 401 before matching ever happens, so "not a 404" would be true of
//! every URL including nonsense - the test would pass against an API that had
//! no routes at all.

use actix_web::http::StatusCode;
use actix_web::{App, test, web};
use conveyor_service::config::ConveyorConfig;
use conveyor_service::providers::Providers;
use conveyor_service::routers::api;
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::Db;
use std::sync::OnceLock;
use tokio::sync::Mutex;

/// `JwtConfig::init` reads the environment, which the whole binary shares, so
/// the two modes take turns rather than racing.
fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

async fn status_with_auth(auth: bool, request: test::TestRequest) -> StatusCode {
    let _guard = lock().lock().await;

    // Every other test in this binary that builds a `JwtConfig` via
    // `for_tests()` (which reads `SERVICE_AUTH_ENABLED` at construction, see
    // `quench_auth::actix::domain::jwt::JwtConfig::from_parts`) expects auth
    // to default off. Leaving `true` set here after this function returns
    // would leak into whichever test the binary happens to run next.
    let previous = std::env::var("SERVICE_AUTH_ENABLED").ok();
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", if auth { "true" } else { "false" }) };

    let db = Db::connect("").await.expect("in-memory database");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(db))
            .app_data(web::Data::new(Providers::from_env()))
            .app_data(web::Data::new(ConveyorConfig::default()))
            .service(api::scope(JwtConfig::for_tests())),
    )
    .await;

    let status = test::call_service(&app, request.to_request())
        .await
        .status();

    match previous {
        Some(value) => unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", value) },
        None => unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") },
    }

    status
}

/// Every route behind the realm's auth, and a request shaped to reach it.
fn authenticated_routes() -> Vec<(&'static str, test::TestRequest)> {
    vec![
        ("GET /repos", test::TestRequest::get().uri("/api/v1/repos")),
        (
            "POST /repos",
            test::TestRequest::post()
                .uri("/api/v1/repos")
                .set_json(serde_json::json!({
                    "owner": "o", "name": "n", "clone_url": "file:///tmp/x"
                })),
        ),
        (
            "GET /repos/{id}",
            test::TestRequest::get().uri("/api/v1/repos/abc"),
        ),
        (
            "PATCH /repos/{id}",
            test::TestRequest::patch()
                .uri("/api/v1/repos/abc")
                .set_json(serde_json::json!({ "enabled": true })),
        ),
        (
            "POST /repos/{id}/enabled",
            test::TestRequest::post()
                .uri("/api/v1/repos/abc/enabled")
                .set_json(serde_json::json!({ "enabled": false })),
        ),
        (
            "DELETE /repos/{id}",
            test::TestRequest::delete().uri("/api/v1/repos/abc"),
        ),
        (
            "POST /repos/{id}/runs",
            test::TestRequest::post()
                .uri("/api/v1/repos/abc/runs")
                .set_json(serde_json::json!({})),
        ),
        ("GET /runs", test::TestRequest::get().uri("/api/v1/runs")),
        (
            "GET /runs/{id}",
            test::TestRequest::get().uri("/api/v1/runs/abc"),
        ),
        (
            "POST /runs/{id}/cancel",
            test::TestRequest::post().uri("/api/v1/runs/abc/cancel"),
        ),
        (
            "GET /jobs/{id}/logs",
            test::TestRequest::get().uri("/api/v1/jobs/abc/logs"),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn every_declared_route_resolves_to_a_handler() {
    for (name, request) in authenticated_routes() {
        assert_ne!(
            status_with_auth(false, request).await,
            StatusCode::NOT_FOUND,
            "{name} did not resolve"
        );
    }
}

#[actix_web::test]
async fn triggering_a_run_resolves() {
    // The one that was broken, on its own so a failure names it.
    assert_ne!(
        status_with_auth(
            false,
            test::TestRequest::post()
                .uri("/api/v1/repos/some-id/runs")
                .set_json(serde_json::json!({})),
        )
        .await,
        StatusCode::NOT_FOUND
    );
}

#[actix_web::test]
async fn an_undeclared_route_is_a_404() {
    // The check that keeps the tests above from being tautologies.
    assert_eq!(
        status_with_auth(false, test::TestRequest::get().uri("/api/v1/nonsense")).await,
        StatusCode::NOT_FOUND
    );
}

#[actix_web::test]
async fn an_unknown_provider_is_a_404() {
    assert_eq!(
        status_with_auth(
            false,
            test::TestRequest::post()
                .uri("/api/v1/webhooks/gitlab")
                .set_payload("{}"),
        )
        .await,
        StatusCode::NOT_FOUND
    );
}

#[actix_web::test]
async fn the_queue_refuses_an_in_memory_database_rather_than_using_it() {
    // A queue on top of one would look like it worked and lose every queued run
    // on restart.
    assert_eq!(
        status_with_auth(false, test::TestRequest::get().uri("/api/v1/runs")).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn the_api_needs_a_token() {
    for (name, request) in authenticated_routes() {
        assert_eq!(
            status_with_auth(true, request).await,
            StatusCode::UNAUTHORIZED,
            "{name} should have required a token"
        );
    }
}

#[actix_web::test]
async fn webhooks_are_reachable_without_a_token() {
    // A provider has no realm token; its delivery is authenticated by its
    // signature instead.
    let status = status_with_auth(
        true,
        test::TestRequest::post()
            .uri("/api/v1/webhooks/github")
            .set_payload("{}"),
    )
    .await;

    assert_ne!(status, StatusCode::NOT_FOUND, "the route should resolve");
    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "webhooks must not sit behind the realm's auth"
    );
}
