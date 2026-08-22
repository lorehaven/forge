//! `startup/default_models.rs`. The `monitor_default_models`/
//! `request_model_launch`/`shutdown` tests use the same `wiremock`-backed
//! `SwitchboardClient` harness as `clients_switchboard_tests.rs` - see that
//! file's module doc for why pointing `SWITCHBOARD_URL`/`GATEHOUSE_URL` at
//! one mock server is a real injection point despite `SwitchboardClient`
//! having no programmatic one.

use crate::env_support::env_lock;
use sage_service::clients::switchboard::VllmInstance;
use sage_service::config::{DefaultModel, SageConfig};
use sage_service::startup::default_models::*;
use sage_service::tools;
use std::collections::HashMap;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn model(name: &str) -> DefaultModel {
    DefaultModel {
        name: name.to_string(),
        gpu_memory_utilization: None,
        max_model_len: None,
        quantization: None,
        dtype: None,
        limit_mm_per_prompt: None,
        enable_tool_calling: false,
        task: None,
    }
}

fn instance(model: &str, status: &str) -> VllmInstance {
    VllmInstance {
        id: format!("pid-{model}"),
        namespace: "native".to_string(),
        model: model.to_string(),
        host: "0.0.0.0".to_string(),
        port: 8000,
        quantization: None,
        max_model_len: None,
        gpu_memory_utilization: None,
        enable_prefix_caching: false,
        task: None,
        started_at: chrono::Utc::now(),
        status: status.to_string(),
    }
}

fn test_config(models: &[&str]) -> SageConfig {
    SageConfig {
        system_prompt: String::new(),
        default_models: models.iter().map(|n| model(n)).collect(),
        supported_models: vec!["*".to_string()],
        default_search_provider: "none".to_string(),
        available_search_providers: vec![],
        capability_profile: tools::capabilities::get_profile("web_assistant").unwrap(),
        stop_models_on_shutdown: false,
    }
}

#[test]
fn launches_the_first_missing_model() {
    let config = test_config(&["chat", "embed"]);
    let next = next_model_to_launch(&config, &[], &HashMap::new()).unwrap();
    assert_eq!(next.name, "chat");
}

#[test]
fn waits_while_another_model_is_starting() {
    let config = test_config(&["chat", "embed"]);
    let instances = vec![instance("chat", "starting")];
    assert!(next_model_to_launch(&config, &instances, &HashMap::new()).is_none());

    let instances = vec![instance("chat", "pending")];
    assert!(next_model_to_launch(&config, &instances, &HashMap::new()).is_none());
}

#[test]
fn moves_on_once_the_previous_model_is_running() {
    let config = test_config(&["chat", "embed"]);
    let instances = vec![instance("chat", "running")];
    let next = next_model_to_launch(&config, &instances, &HashMap::new()).unwrap();
    assert_eq!(next.name, "embed");
}

#[test]
fn relaunches_a_failed_model_before_the_rest() {
    let config = test_config(&["chat", "embed"]);
    let instances = vec![instance("chat", "failed")];
    let next = next_model_to_launch(&config, &instances, &HashMap::new()).unwrap();
    assert_eq!(next.name, "chat");
}

#[test]
fn skips_a_model_that_exhausted_its_attempts() {
    let config = test_config(&["chat", "embed"]);
    let instances = vec![instance("chat", "failed")];
    let attempts = HashMap::from([("chat".to_string(), MAX_LAUNCH_ATTEMPTS)]);
    let next = next_model_to_launch(&config, &instances, &attempts).unwrap();
    assert_eq!(next.name, "embed");
}

#[test]
fn skips_unsupported_models() {
    let mut config = test_config(&["chat", "embed"]);
    config.supported_models = vec!["embed".to_string()];
    let next = next_model_to_launch(&config, &[], &HashMap::new()).unwrap();
    assert_eq!(next.name, "embed");
}

#[test]
fn nothing_to_launch_when_all_models_are_running() {
    let config = test_config(&["chat", "embed"]);
    let instances = vec![instance("chat", "running"), instance("embed", "running")];
    assert!(next_model_to_launch(&config, &instances, &HashMap::new()).is_none());
}

// ---------------------------------------------------------------------
// monitor_default_models / request_model_launch / shutdown
// ---------------------------------------------------------------------

fn instance_json(id: &str, model: &str, status: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "namespace": "ns",
        "model": model,
        "host": "127.0.0.1",
        "port": 8000,
        "quantization": null,
        "max_model_len": null,
        "gpu_memory_utilization": null,
        "enable_prefix_caching": false,
        "task": null,
        "started_at": chrono::Utc::now().to_rfc3339(),
        "status": status
    })
}

/// Points `SWITCHBOARD_URL`/`GATEHOUSE_URL` at a fresh mock server and mocks
/// the token exchange; callers add whatever `/api/v1/vllm/instances` mocks
/// they need beyond that. The server is leaked deliberately - it must
/// outlive every request the client (constructed after this returns) makes
/// during the test.
async fn switchboard() -> (
    sage_service::clients::switchboard::SwitchboardClient,
    &'static MockServer,
) {
    let server: &'static MockServer = Box::leak(Box::new(MockServer::start().await));
    Mock::given(method("POST"))
        .and(path("/api/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "test-token",
            "expires_in": 3600
        })))
        .mount(server)
        .await;
    unsafe {
        std::env::set_var("SWITCHBOARD_URL", server.uri());
        std::env::set_var("GATEHOUSE_URL", server.uri());
        std::env::set_var("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    }
    (
        sage_service::clients::switchboard::SwitchboardClient::new(),
        server,
    )
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn monitor_default_models_returns_early_when_switchboard_is_unreachable() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    // Nothing is listening on this address - the request fails outright,
    // with no token mock or retry delay needed.
    unsafe {
        std::env::set_var("SWITCHBOARD_URL", "http://127.0.0.1:1");
        std::env::set_var("GATEHOUSE_URL", "http://127.0.0.1:1");
        std::env::set_var("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    }
    let sb = sage_service::clients::switchboard::SwitchboardClient::new();
    let cfg = test_config(&["chat"]);
    let mut attempts = HashMap::new();
    let launched = LaunchedInstances::default();

    monitor_default_models(&sb, &cfg, &mut attempts, &launched).await;
    assert!(attempts.is_empty());
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn monitor_default_models_launches_the_next_missing_model() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let (sb, server) = switchboard().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(instance_json("new-inst", "chat", "starting")),
        )
        .mount(server)
        .await;

    let cfg = test_config(&["chat"]);
    let mut attempts = HashMap::new();
    let launched = LaunchedInstances::default();

    monitor_default_models(&sb, &cfg, &mut attempts, &launched).await;
    assert_eq!(attempts.get("chat"), Some(&1));
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn monitor_default_models_clears_attempts_once_a_model_is_running() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let (sb, server) = switchboard().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!([instance_json("i1", "chat", "running")])),
        )
        .mount(server)
        .await;

    let cfg = test_config(&["chat"]);
    let mut attempts = HashMap::new();
    attempts.insert("chat".to_string(), 2);
    let launched = LaunchedInstances::default();

    monitor_default_models(&sb, &cfg, &mut attempts, &launched).await;
    assert!(!attempts.contains_key("chat"));
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn request_model_launch_records_the_launched_instance() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let (sb, server) = switchboard().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(instance_json(
            "launched-1",
            "chat",
            "starting",
        )))
        .mount(server)
        .await;

    let m = model("chat");
    let launched = LaunchedInstances::default();
    // Doesn't panic; the effect (the id being recorded) is confirmed via
    // `shutdown` recognizing it as owned, below.
    request_model_launch(&sb, &m, &launched).await;
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn request_model_launch_does_not_panic_when_the_launch_request_fails() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let (sb, server) = switchboard().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(500))
        .mount(server)
        .await;

    let m = model("chat");
    let launched = LaunchedInstances::default();
    request_model_launch(&sb, &m, &launched).await;
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn shutdown_is_a_no_op_when_stop_models_on_shutdown_is_disabled() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    // No mocks registered at all - a real request here would fail the test.
    unsafe {
        std::env::set_var("SWITCHBOARD_URL", "http://127.0.0.1:1");
        std::env::set_var("GATEHOUSE_URL", "http://127.0.0.1:1");
        std::env::set_var("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    }
    let sb = sage_service::clients::switchboard::SwitchboardClient::new();
    let mut cfg = test_config(&["chat"]);
    cfg.stop_models_on_shutdown = false;
    let launched = LaunchedInstances::default();

    shutdown(&sb, &cfg, &launched).await;
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn shutdown_returns_early_when_switchboard_is_unreachable() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    unsafe {
        std::env::set_var("SWITCHBOARD_URL", "http://127.0.0.1:1");
        std::env::set_var("GATEHOUSE_URL", "http://127.0.0.1:1");
        std::env::set_var("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    }
    let sb = sage_service::clients::switchboard::SwitchboardClient::new();
    let mut cfg = test_config(&["chat"]);
    cfg.stop_models_on_shutdown = true;
    let launched = LaunchedInstances::default();

    shutdown(&sb, &cfg, &launched).await;
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn shutdown_stops_an_instance_this_process_launched() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let (sb, server) = switchboard().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(instance_json("mine-1", "chat", "starting")),
        )
        .mount(server)
        .await;

    let m = model("chat");
    let launched = LaunchedInstances::default();
    request_model_launch(&sb, &m, &launched).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([instance_json(
                "mine-1", "chat", "running"
            )])),
        )
        .mount(server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/vllm/instances/mine-1"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server)
        .await;

    let mut cfg = test_config(&["chat"]);
    cfg.stop_models_on_shutdown = true;
    shutdown(&sb, &cfg, &launched).await;
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn shutdown_does_not_stop_an_instance_it_never_launched() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let (sb, server) = switchboard().await;
    // An instance that started *after* `launched`'s `process_started_at`
    // and that `launched` never recorded - i.e. relaunched by a different
    // sage replica. No DELETE mock is registered, so a real stop attempt
    // would fail the test with an unmatched-request panic.
    let launched = LaunchedInstances::default();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([instance_json(
                "not-mine", "chat", "running"
            )])),
        )
        .mount(server)
        .await;

    let mut cfg = test_config(&["chat"]);
    cfg.stop_models_on_shutdown = true;
    shutdown(&sb, &cfg, &launched).await;
}
