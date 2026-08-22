//! `get_status` (REST) and `handle_sse` (SSE) under `routers/gpu`.

use crate::env_support::env_lock;
use actix_web::web::Data;
use actix_web::{App, test};
use quench_auth::prelude::JwtConfig;
use switchboard_service::routers::gpu::{self, GpuBroadcaster, get_status, handle_sse};

#[actix_web::test]
async fn get_status_returns_gpu_info_as_json() {
    let app = test::init_service(App::new().service(get_status)).await;
    let req = test::TestRequest::get().uri("/status").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // `GpuInfo` only derives `Serialize` (it's a broadcast payload, never
    // deserialized elsewhere in this crate), so assert on the JSON shape
    // directly rather than via `read_body_json::<GpuInfo>`.
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body.get("total_gb").and_then(|v| v.as_f64()).is_some());
    assert!(body.get("free_gb").and_then(|v| v.as_f64()).is_some());
}

#[actix_web::test]
async fn gpu_sse_route_streams_a_broadcast_message() {
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let data = Data::new(GpuBroadcaster(tx.clone()));
    let app = test::init_service(App::new().app_data(data).service(handle_sse)).await;

    let req = test::TestRequest::get().uri("/status/sse").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/event-stream"
    );

    tx.send("<div>gpu</div>".to_string()).unwrap();
    // Same reasoning as the vLLM SSE tests: drop every sender so the
    // never-ending broadcast stream actually closes before `read_body` waits
    // for end-of-stream.
    drop(app);
    drop(tx);
    let body = test::read_body(resp).await;
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("event: gpu-status"));
    assert!(text.contains("<div>gpu</div>"));
}

#[actix_web::test]
async fn gpu_scope_mounts_both_routes_under_api_v1_gpu() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::set_var("SERVICE_AUTH_ENABLED", "false") };

    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let data = Data::new(GpuBroadcaster(tx));
    let app = test::init_service(
        App::new()
            .app_data(data)
            .service(gpu::scope(JwtConfig::for_tests())),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/gpu/status")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    unsafe { std::env::remove_var("SERVICE_AUTH_ENABLED") };
}
