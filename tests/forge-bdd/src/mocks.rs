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

/// Starts a stand-in for `node_exporter` on 19100, for Gauge's suite: two cores, memory, one disk and a load.
pub async fn start_node_exporter() {
    println!("Starting mock node_exporter...");
    tokio::spawn(serve("node_exporter", 19100, Arc::new(NodeExporter)));
    sleep(Duration::from_millis(300)).await;
}

struct NodeExporter;

const NODE_METRICS: &str = r#"# HELP node_cpu_seconds_total Seconds the CPUs spent in each mode.
# TYPE node_cpu_seconds_total counter
node_cpu_seconds_total{cpu="0",mode="idle"} 9000
node_cpu_seconds_total{cpu="0",mode="user"} 700
node_cpu_seconds_total{cpu="0",mode="system"} 300
node_cpu_seconds_total{cpu="1",mode="idle"} 9500
node_cpu_seconds_total{cpu="1",mode="user"} 400
node_cpu_seconds_total{cpu="1",mode="system"} 100
# TYPE node_memory_MemTotal_bytes gauge
node_memory_MemTotal_bytes 17179869184
# TYPE node_memory_MemAvailable_bytes gauge
node_memory_MemAvailable_bytes 8589934592
# TYPE node_memory_MemFree_bytes gauge
node_memory_MemFree_bytes 4294967296
# TYPE node_load1 gauge
node_load1 0.42
node_load5 0.30
node_load15 0.20
# TYPE node_boot_time_seconds gauge
node_boot_time_seconds 1700000000
# TYPE node_filesystem_size_bytes gauge
node_filesystem_size_bytes{device="/dev/sda1",fstype="ext4",mountpoint="/"} 107374182400
# TYPE node_filesystem_avail_bytes gauge
node_filesystem_avail_bytes{device="/dev/sda1",fstype="ext4",mountpoint="/"} 53687091200
"#;

#[async_trait]
impl Endpoint for NodeExporter {
    async fn call(&self, req: Request) -> Response {
        if req.method() == Method::GET && req.uri().path() == "/metrics" {
            return Response::text(StatusCode::OK, NODE_METRICS);
        }
        Response::not_found()
    }
}
