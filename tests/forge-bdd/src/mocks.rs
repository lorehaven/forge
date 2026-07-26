//! Mock switchboard and vLLM backends for the sage suite, so it does not need
//! the real services to run.

use tokio::time::{Duration, sleep};

/// Starts both mocks and waits for them to accept connections.
pub async fn start() {
    println!("Starting mock switchboard and vLLM servers...");
    tokio::spawn(start_mock_switchboard_server());
    tokio::spawn(start_mock_vllm_server());
    sleep(Duration::from_millis(500)).await;
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
            crate::steps::sage::shutdown::record_deleted_instance(id);

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
                "port": 18000,
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

    let addr = ([127, 0, 0, 1], 18000).into();
    if let Err(e) = Server::bind(&addr).serve(make_svc).await {
        eprintln!("Mock vLLM server error: {}", e);
    }
}
