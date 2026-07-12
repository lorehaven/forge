use crate::steps::common::SageWorld;
use cucumber::World;
use std::env;
use tokio::process::Command;
use tokio::time::{Duration, sleep};

mod steps;

#[tokio::main]
async fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let env_path = std::path::Path::new(manifest_dir).join(".env");
    dotenvy::from_path(env_path).ok();

    // 1. Start mock Switchboard server
    println!("Starting mock Switchboard server...");
    tokio::spawn(start_mock_switchboard_server());
    sleep(Duration::from_millis(500)).await;

    // 2. Start mock vLLM server
    println!("Starting mock vLLM server...");
    tokio::spawn(start_mock_vllm_server());
    sleep(Duration::from_millis(500)).await;

    // 3. Start sage-service
    let workspace_root = std::path::Path::new(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let sage_service_dir = workspace_root.join("docker").join("sage-service");

    // Build first, then run the binary directly (instead of `cargo run`) so the
    // spawned PID is sage-service itself. The shutdown scenario sends it a
    // SIGTERM, which a `cargo run` parent would not reliably forward.
    println!("Building sage-service...");
    let build_status = Command::new("cargo")
        .arg("build")
        .arg("-p")
        .arg("sage-service")
        .current_dir(&sage_service_dir)
        .envs(env::vars())
        .status()
        .await
        .expect("Failed to build sage-service");
    assert!(build_status.success(), "sage-service build failed");

    let target_dir = env::var("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| workspace_root.join("target"));
    let sage_bin = target_dir.join("debug").join("sage-service");

    println!("Starting sage-service...");
    let mut child = Command::new(&sage_bin)
        .current_dir(&sage_service_dir)
        .envs(env::vars())
        .env("SERVER_ADDR", "127.0.0.1:7777")
        .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:7778")
        .env("SERVER_CERT_PATH", "cert.pem")
        .env("SERVER_KEY_PATH", "key.pem")
        // Disable switchboard dependency check
        .env("SKIP_SWITCHBOARD_CHECK", "true")
        .env("VLLM_TLS_VERIFY", "false")
        // Point to mock vLLM server
        .env("VLLM_BASE_URL", "http://127.0.0.1:8000")
        // Exercise the graceful default-model teardown on shutdown.
        .env("SAGE_STOP_MODELS_ON_SHUTDOWN", "true")
        .spawn()
        .expect("Failed to start sage-service");

    // Record the PID so the shutdown scenario can signal this exact process.
    steps::shutdown::set_sage_pid(child.id().expect("sage-service PID unavailable"));

    // Wait for service to be ready
    println!("Waiting for service to start...");
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    for attempt in 1..=40 {
        sleep(Duration::from_millis(500)).await;
        match client
            .get("https://localhost:7777/sage/ui/login")
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 401 => {
                println!("Sage service ready!");
                break;
            }
            _ => {
                if attempt % 5 == 0 {
                    println!("Waiting for service... ({}s)", attempt / 2);
                }
            }
        }
    }

    // Run tests. The shutdown scenario terminates the service under test, and
    // cucumber runs scenarios concurrently, so it can't share a run with the
    // others. Run the rest of the suite first, then the shutdown scenario alone
    // against the still-live service.
    let features_path = std::path::Path::new(manifest_dir).join("features");
    const SHUTDOWN_FEATURE: &str = "Graceful shutdown";

    SageWorld::filter_run(features_path.clone(), |feature, _, _| {
        feature.name != SHUTDOWN_FEATURE
    })
    .await;

    SageWorld::filter_run(features_path, |feature, _, _| {
        feature.name == SHUTDOWN_FEATURE
    })
    .await;

    // Stop service (harmless if the shutdown scenario already terminated it).
    println!("Stopping sage-service...");
    let _ = child.kill().await;
}

async fn start_mock_switchboard_server() {
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Request, Response, Server, StatusCode};
    use std::convert::Infallible;

    async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let path = req.uri().path();

        // Check basic auth
        let has_auth = req
            .headers()
            .get(hyper::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .map(|auth| auth.starts_with("Basic "))
            .unwrap_or(false);

        eprintln!(
            "Switchboard mock: {} {} (auth={})",
            req.method(),
            path,
            has_auth
        );

        if !has_auth {
            return Ok(Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::from("Unauthorized"))
                .unwrap());
        }

        // Graceful stop of an instance: DELETE /vllm/instances/{id}. The real
        // switchboard responds with an HTML fragment; record the id so the
        // shutdown scenario can assert sage asked for the stop.
        if req.method() == hyper::Method::DELETE
            && path.starts_with("/switchboard/api/v1/vllm/instances/")
        {
            let id = path.rsplit('/').next().unwrap_or_default().to_string();
            crate::steps::shutdown::record_deleted_instance(id);

            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/html")
                .body(Body::from("<div></div>"))
                .unwrap());
        }

        if path == "/switchboard/api/v1/vllm/instances" && req.method() == hyper::Method::GET {
            let response = serde_json::json!([{
                "id": "mock-1782724283792",
                "namespace": "default",
                "model": "test-model",
                "host": "127.0.0.1",
                "port": 8000,
                "quantization": null,
                "max_model_len": 4096,
                "gpu_memory_utilization": 0.5,
                "enable_prefix_caching": false,
                "started_at": "2026-07-01T00:00:00Z",
                "status": "running"
            }]);

            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Body::from(response.to_string()))
                .unwrap());
        }

        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap())
    }

    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle_request)) });

    let addr = ([127, 0, 0, 1], 19554).into();
    eprintln!("Mock Switchboard server binding to {:?}", addr);
    if let Err(e) = Server::bind(&addr).serve(make_svc).await {
        eprintln!("Mock Switchboard server error: {}", e);
    }
    eprintln!("Mock Switchboard server exited");
}

async fn start_mock_vllm_server() {
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Request, Response, Server, StatusCode};
    use std::convert::Infallible;

    async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let path = req.uri().path();

        if path == "/v1/chat/completions" {
            let response = serde_json::json!({
                "id": "mock-completion",
                "object": "chat.completion",
                "created": 1234567890,
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
            });

            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Body::from(response.to_string()))
                .unwrap());
        }

        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap())
    }

    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle_request)) });

    let addr = ([127, 0, 0, 1], 8000).into();
    if let Err(e) = Server::bind(&addr).serve(make_svc).await {
        eprintln!("Mock vLLM server error: {}", e);
    }
}
