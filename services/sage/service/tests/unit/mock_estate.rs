//! One mock server that stands in for gatehouse, switchboard and vLLM (and any web page a test
//! serves from it), for the tests that run the real HTTP clients end to end.
//!
//! `mock_estate` points the process-global `SWITCHBOARD_URL` / `GATEHOUSE_URL` at it, so callers
//! hold `env_support::env_lock` for the whole test.

use std::fmt::Write as _;

use quench_test::mock::matchers::{body_string_contains, method, path};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};
use sage_service::clients::switchboard::{SwitchboardClient, VllmInstance};

pub const EMBEDDING_MODEL: &str = "Qwen/Qwen3-Embedding-0.6B";
pub const CHAT_MODEL: &str = "chat-model";

pub fn json(status: u16, body: &quench_serde::Value) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_raw(quench_serde::json::to_string(body), "application/json")
}

/// A chat completion whose message content is `content`.
pub fn completion(content: &str) -> ResponseTemplate {
    json(
        200,
        &quench_serde::json!({"choices": [{"message": {"content": content}}]}),
    )
}

/// A streamed chat completion made of one content delta per entry of `deltas`.
pub fn sse(deltas: &[&str]) -> ResponseTemplate {
    let mut body = String::new();
    for delta in deltas {
        let chunk = quench_serde::json!({"choices": [{"index": 0, "delta": {"content": delta}}]});
        let _ = write!(body, "data: {}\n\n", quench_serde::json::to_string(&chunk));
    }
    body.push_str("data: [DONE]\n\n");
    ResponseTemplate::new(200).set_body_raw(body, "text/event-stream")
}

pub fn instance_json(model: &str, port: u16, task: Option<&str>) -> quench_serde::Value {
    quench_serde::json!({
        "id": format!("id-{model}"),
        "namespace": "ns",
        "model": model,
        "host": "127.0.0.1",
        "port": port,
        "quantization": null,
        "max_model_len": null,
        "gpu_memory_utilization": null,
        "enable_prefix_caching": false,
        "task": task,
        "started_at": quench_core::time::Time::now().to_rfc3339(),
        "status": "running"
    })
}

/// The chat instance (id `id-chat-model`) served by `server`.
pub fn answerer(server: &MockServer) -> VllmInstance {
    quench_serde::json::from_str(&quench_serde::json::to_string(&instance_json(
        CHAT_MODEL,
        server.address().port(),
        None,
    )))
    .expect("an instance")
}

/// Mounts the token endpoint and a switchboard that lists the embedding model, the chat model and
/// `extra` models, all served by the mock itself.
pub async fn mock_estate(extra: &[&str]) -> (MockServer, SwitchboardClient) {
    let server = MockServer::start().await;
    let port = server.address().port();
    Mock::given(method("POST"))
        .and(path("/api/v1/token"))
        .respond_with(json(
            200,
            &quench_serde::json!({"access_token": "t", "expires_in": 3600}),
        ))
        .mount(&server)
        .await;
    let mut instances = vec![
        instance_json(EMBEDDING_MODEL, port, Some("embed")),
        instance_json(CHAT_MODEL, port, None),
    ];
    instances.extend(extra.iter().map(|m| instance_json(m, port, None)));
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(json(200, &quench_serde::Value::Array(instances)))
        .mount(&server)
        .await;
    quench_core::env::set("SWITCHBOARD_URL", server.uri());
    quench_core::env::set("GATEHOUSE_URL", server.uri());
    quench_core::env::set("CLIENT_SECRET_SAGE_SWITCHBOARD", "secret");
    quench_core::env::set("SAGE_EMBEDDING_MODEL", EMBEDDING_MODEL);
    let switchboard = SwitchboardClient::new();
    (server, switchboard)
}

/// Answers chat completions whose request mentions `marker` with `response`.
pub async fn on_chat(server: &MockServer, marker: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(marker))
        .respond_with(response)
        .mount(server)
        .await;
}
