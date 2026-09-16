use crate::domain::docker_error;
use crate::routers::docker::{blob_path, validate_digest};
use quench_http::prelude::{
    Endpoint, FromRequest, Path, Request, Response,
    http::{Method, StatusCode},
};
use quench_starter::http::domain::error;

// Hand-expanded `#[head(...)]` (the macro doesn't exist). Pattern must match `retrieve.rs`'s `#[get(...)]`
// byte-for-byte - quench-http keys routes by literal pattern string, so a mismatch registers a second resource.
pub async fn handle(Path((_repo, digest)): Path<(String, String)>) -> Response {
    if !validate_digest(&digest) {
        return error::response(
            StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "invalid digest",
        );
    }

    let Some(blob_path) = blob_path(&digest) else {
        return error::response(
            StatusCode::BAD_REQUEST,
            error::UNSUPPORTED,
            "invalid digest",
        );
    };
    match std::fs::metadata(&blob_path) {
        Ok(metadata) => Response::new(StatusCode::OK)
            .header("content-type", "application/octet-stream")
            .header("docker-content-digest", &digest)
            .header("content-length", metadata.len().to_string())
            .header("accept-ranges", "bytes"),
        Err(_) => error::response(
            StatusCode::NOT_FOUND,
            docker_error::BLOB_UNKNOWN,
            "blob unknown to registry",
        ),
    }
}

#[allow(non_camel_case_types)]
struct __quench_route_handle_head;

#[async_trait::async_trait]
impl Endpoint for __quench_route_handle_head {
    async fn call(&self, mut req: Request) -> Response {
        let arg0 = match <Path<(String, String)> as FromRequest>::from_request(&mut req).await {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        handle(arg0).await
    }
}

quench_http::inventory::submit! {
    quench_http::prelude::RouteRegistration {
        method: Method::HEAD,
        pattern: "/v2/{name:.+}/blobs/{digest}",
        endpoint: || std::sync::Arc::new(__quench_route_handle_head) as std::sync::Arc<dyn Endpoint>,
    }
}

pub fn register_routes() {
    let _ = handle as fn(_) -> _;
}
