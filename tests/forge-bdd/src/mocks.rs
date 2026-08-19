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
    use http_body_util::Full;
    use hyper::body::{Bytes, Incoming};
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response, StatusCode};
    use hyper_util::rt::TokioIo;
    use std::convert::Infallible;
    use tokio::net::TcpListener;

    async fn handle_request(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
        let path = req.uri().path();

        // sage authenticates with a gatehouse-issued bearer token now
        // (client_credentials), not HTTP Basic - see
        // `docker/sage-service/src/clients/switchboard.rs`.
        let has_auth = req
            .headers()
            .get(hyper::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .map(|auth| auth.starts_with("Bearer "))
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
                .body(Full::new(Bytes::from("Unauthorized")))
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
                .body(Full::new(Bytes::from("<div></div>")))
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
                .body(Full::new(Bytes::from(response.to_string())))
                .unwrap());
        }

        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from("Not Found")))
            .unwrap())
    }

    let addr: std::net::SocketAddr = ([127, 0, 0, 1], 19554).into();
    eprintln!("Mock Switchboard server binding to {:?}", addr);
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("Mock Switchboard server failed to bind: {}", e);
            return;
        }
    };

    loop {
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Mock Switchboard server accept error: {}", e);
                continue;
            }
        };
        let io = TokioIo::new(stream);
        tokio::spawn(async move {
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, service_fn(handle_request))
                .await
            {
                eprintln!("Mock Switchboard server error: {}", e);
            }
        });
    }
}

async fn start_mock_vllm_server() {
    use http_body_util::Full;
    use hyper::body::{Bytes, Incoming};
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response, StatusCode};
    use hyper_util::rt::TokioIo;
    use std::convert::Infallible;
    use tokio::net::TcpListener;

    async fn handle_request(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
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
                .body(Full::new(Bytes::from(response.to_string())))
                .unwrap());
        }

        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from("Not Found")))
            .unwrap())
    }

    let addr: std::net::SocketAddr = ([127, 0, 0, 1], 18000).into();
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("Mock vLLM server failed to bind: {}", e);
            return;
        }
    };

    loop {
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Mock vLLM server accept error: {}", e);
                continue;
            }
        };
        let io = TokioIo::new(stream);
        tokio::spawn(async move {
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, service_fn(handle_request))
                .await
            {
                eprintln!("Mock vLLM server error: {}", e);
            }
        });
    }
}
