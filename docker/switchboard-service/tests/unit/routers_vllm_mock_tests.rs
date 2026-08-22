//! `MockVllmEngine`'s in-process store: launch inserts, list reflects it,
//! stop removes it (and reports the instance as gone the second time).
//!
//! `MOCK_INSTANCES` is a process-global static keyed by `mock-{millisecond
//! timestamp}`, and `init_engine_selects_mock_mode` (routers_vllm_mod_tests)
//! also touches it - every test here holds `mock_lock` for its duration so
//! two launches never collide on the same millisecond's key.

use crate::env_support::mock_lock;
use switchboard_service::routers::vllm::engine::VllmEngine;
use switchboard_service::routers::vllm::mock::MockVllmEngine;
use switchboard_service::routers::vllm::types::LaunchRequest;

fn request(model: &str) -> LaunchRequest {
    LaunchRequest {
        model: model.to_string(),
        host: "0.0.0.0".to_string(),
        port: 8000,
        namespace: None,
        quantization: None,
        dtype: None,
        limit_mm_per_prompt: None,
        max_model_len: None,
        gpu_memory_utilization: None,
        enable_prefix_caching: false,
        enable_tool_calling: false,
        task: None,
    }
}

#[tokio::test]
async fn launch_then_list_then_stop_round_trips_through_the_mock_store() {
    let _guard = mock_lock().lock().await;
    let engine = MockVllmEngine;

    let launched = engine
        .launch_instance(request("mock-round-trip-model"))
        .await
        .expect("mock launch never errors");
    assert_eq!(launched.model, "mock-round-trip-model");
    assert_eq!(launched.namespace, "default");
    assert_eq!(launched.status, "running");

    let listed = engine
        .list_instances()
        .await
        .expect("mock list never errors");
    assert!(listed.iter().any(|i| i.id == launched.id));

    engine
        .stop_instance(launched.id.clone())
        .await
        .expect("the instance we just launched is there to stop");

    let after_stop = engine
        .list_instances()
        .await
        .expect("mock list never errors");
    assert!(!after_stop.iter().any(|i| i.id == launched.id));
}

#[tokio::test]
async fn stopping_an_unknown_instance_errors() {
    let _guard = mock_lock().lock().await;
    let engine = MockVllmEngine;
    let err = engine
        .stop_instance("does-not-exist".to_string())
        .await
        .unwrap_err();
    assert!(err.contains("does-not-exist"));
}

#[tokio::test]
async fn launch_honors_an_explicit_namespace() {
    let _guard = mock_lock().lock().await;
    let engine = MockVllmEngine;
    let mut req = request("mock-namespaced-model");
    req.namespace = Some("custom-ns".to_string());

    let launched = engine
        .launch_instance(req)
        .await
        .expect("mock launch never errors");
    assert_eq!(launched.namespace, "custom-ns");

    engine
        .stop_instance(launched.id)
        .await
        .expect("cleanup: the instance we just launched is there to stop");
}
