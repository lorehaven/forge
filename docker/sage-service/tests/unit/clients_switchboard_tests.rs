//! `clients/switchboard.rs`. `SwitchboardClient` has no injectable client,
//! only the `SWITCHBOARD_URL`/`GATEHOUSE_URL`/`CLIENT_SECRET_SAGE_SWITCHBOARD`
//! env vars read once at construction (`SwitchboardClient::new()`) - but
//! that's still an injection point: pointing both `SWITCHBOARD_URL` and
//! `GATEHOUSE_URL` at the same `wiremock` server lets it stand in for both
//! the client-credentials token endpoint (`{GATEHOUSE_URL}/api/v1/token`)
//! and the switchboard API itself, so the real HTTP methods can be
//! exercised end to end without a real gatehouse or switchboard.

use crate::env_support::env_lock;
use chrono::Utc;
use sage_service::clients::switchboard::{SwitchboardClient, VllmInstance};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Points `SWITCHBOARD_URL`/`GATEHOUSE_URL` at `server` and mocks a
/// successful `client_credentials` token exchange, so any `SwitchboardClient`
/// built after this returns reaches `server` for both the token and the API
/// call. Held for the whole test (env vars are process-global) - safe
/// under `cargo nextest`, which gives each test its own process, but the
/// crate-wide `env_lock` is still taken to match this file's own
/// documented convention for anything touching fixed-name env vars.
async fn mock_switchboard() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "test-token",
            "expires_in": 3600
        })))
        .mount(&server)
        .await;

    unsafe {
        std::env::set_var("SWITCHBOARD_URL", server.uri());
        std::env::set_var("GATEHOUSE_URL", server.uri());
        std::env::set_var("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    }
    server
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn get_vllm_instances_parses_a_successful_response() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let server = mock_switchboard().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "id": "inst-1",
                "namespace": "ns",
                "model": "test-model",
                "host": "127.0.0.1",
                "port": 8000,
                "quantization": null,
                "max_model_len": null,
                "gpu_memory_utilization": null,
                "enable_prefix_caching": false,
                "task": null,
                "started_at": Utc::now().to_rfc3339(),
                "status": "running"
            }
        ])))
        .mount(&server)
        .await;

    let client = SwitchboardClient::new();
    let instances = client.get_vllm_instances().await.unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].id, "inst-1");
    assert!(instances[0].is_chat_capable());
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn get_vllm_instances_errors_when_the_token_exchange_is_refused() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/token"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    unsafe {
        std::env::set_var("SWITCHBOARD_URL", server.uri());
        std::env::set_var("GATEHOUSE_URL", server.uri());
        std::env::set_var("CLIENT_SECRET_SAGE_SWITCHBOARD", "test-secret");
    }

    let client = SwitchboardClient::new();
    assert!(client.get_vllm_instances().await.is_err());
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn stop_instance_succeeds_against_a_mocked_delete() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let server = mock_switchboard().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/vllm/instances/inst-1"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let client = SwitchboardClient::new();
    client.stop_instance("inst-1").await.unwrap();
}

#[allow(clippy::await_holding_lock)] // single-threaded test, held deliberately for env-var safety across the whole call
#[tokio::test]
async fn launch_instance_parses_the_created_instance() {
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let server = mock_switchboard().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "inst-2",
            "namespace": "ns",
            "model": "test-model",
            "host": "0.0.0.0",
            "port": 8000,
            "quantization": null,
            "max_model_len": null,
            "gpu_memory_utilization": null,
            "enable_prefix_caching": false,
            "task": null,
            "started_at": Utc::now().to_rfc3339(),
            "status": "starting"
        })))
        .mount(&server)
        .await;

    let client = SwitchboardClient::new();
    let instance = client
        .launch_instance(
            "test-model",
            None,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(instance.id, "inst-2");
}

fn instance(task: Option<&str>) -> VllmInstance {
    VllmInstance {
        id: "id".to_string(),
        namespace: "ns".to_string(),
        model: "model".to_string(),
        host: "host".to_string(),
        port: 8000,
        quantization: None,
        max_model_len: None,
        gpu_memory_utilization: None,
        enable_prefix_caching: false,
        task: task.map(str::to_string),
        device: None,
        started_at: Utc::now(),
        status: "running".to_string(),
    }
}

#[test]
fn chat_instance_with_no_task_is_chat_capable() {
    assert!(instance(None).is_chat_capable());
}

#[test]
fn embed_task_is_not_chat_capable() {
    assert!(!instance(Some("embed")).is_chat_capable());
}

#[test]
fn embedding_task_is_not_chat_capable() {
    assert!(!instance(Some("embedding")).is_chat_capable());
}

#[test]
fn an_unrelated_task_value_is_still_chat_capable() {
    assert!(instance(Some("generate")).is_chat_capable());
}
