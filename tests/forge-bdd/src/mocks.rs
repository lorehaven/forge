//! Mock switchboard and vLLM backends for the sage suite, so it does not need
//! the real services to run.

use quench_core::async_trait;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{Method, StatusCode};
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::server::serve_http_on;
use quench_serde::json;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::time::{Duration, sleep};

/// Starts both mocks and waits for them to accept connections.
pub async fn start() {
    println!("Starting mock switchboard and vLLM servers...");
    tokio::spawn(serve("Switchboard", 19554, Arc::new(Switchboard)));
    tokio::spawn(serve("vLLM", 18000, Arc::new(Vllm)));
    sleep(Duration::from_millis(500)).await;
}

async fn serve(name: &'static str, port: u16, app: Arc<dyn Endpoint>) {
    eprintln!("Mock {name} server binding to 127.0.0.1:{port}");
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("Mock {name} server failed to bind: {e}");
            return;
        }
    };
    let container = match ContainerBuilder::new().build().await {
        Ok(container) => Arc::new(container),
        Err(e) => {
            eprintln!("Mock {name} server could not start: {e}");
            return;
        }
    };
    if let Err(e) = serve_http_on(app, container, listener, std::future::pending()).await {
        eprintln!("Mock {name} server error: {e}");
    }
}

struct Switchboard;

#[async_trait]
impl Endpoint for Switchboard {
    async fn call(&self, req: Request) -> Response {
        let path = req.uri().path().to_string();

        // sage authenticates with a gatehouse-issued bearer token now
        // (client_credentials), not HTTP Basic - see
        // `services/sage/service/src/clients/switchboard.rs`.
        let has_auth = req
            .headers()
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|auth| auth.starts_with("Bearer "));

        eprintln!(
            "Switchboard mock: {} {} (auth={})",
            req.method(),
            path,
            has_auth
        );

        if !has_auth {
            return Response::text(StatusCode::UNAUTHORIZED, "Unauthorized");
        }

        // Graceful stop of an instance: DELETE /vllm/instances/{id}. The real
        // switchboard responds with an HTML fragment; record the id so the
        // shutdown scenario can assert sage asked for the stop.
        if req.method() == Method::DELETE && path.starts_with("/switchboard/api/v1/vllm/instances/")
        {
            let id = path.rsplit('/').next().unwrap_or_default().to_string();
            crate::steps::sage::shutdown::record_deleted_instance(id);

            return Response::html(StatusCode::OK, "<div></div>");
        }

        if path == "/switchboard/api/v1/vllm/instances" && req.method() == Method::GET {
            return Response::json(
                StatusCode::OK,
                &json!([{
                    "id": "mock-1782724283792",
                    "namespace": "default",
                    "model": "test-model",
                    "host": "127.0.0.1",
                    "port": 18000,
                    "quantization": null,
                    "max_model_len": 4096,
                    "gpu_memory_utilization": 0.5,
                    "enable_prefix_caching": false,
                    "started_at": "2026-07-01T00:00:00Z",
                    "status": "running"
                }]),
            )
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
        }

        Response::text(StatusCode::NOT_FOUND, "Not Found")
    }
}

struct Vllm;

#[async_trait]
impl Endpoint for Vllm {
    async fn call(&self, req: Request) -> Response {
        if req.uri().path() == "/v1/chat/completions" {
            return Response::json(
                StatusCode::OK,
                &json!({
                    "id": "mock-completion",
                    "object": "chat.completion",
                    "created": 1_234_567_890,
                    "model": "test-model",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "This is a mock response from the vLLM server."
                        },
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 10,
                        "completion_tokens": 20,
                        "total_tokens": 30
                    }
                }),
            )
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));
        }

        Response::text(StatusCode::NOT_FOUND, "Not Found")
    }
}
