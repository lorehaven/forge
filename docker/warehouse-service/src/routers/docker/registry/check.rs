use quench_http::prelude::{
    Endpoint, Request, Response, get,
    http::{Method, StatusCode},
};

#[get("/v2/")]
pub async fn handle_get() -> Response {
    respond()
}

// No `#[head]` macro exists - hand-expanded from `route_impl` in quench-http-macros.
#[allow(non_camel_case_types)]
struct __quench_route_handle_head;

#[quench_core::async_trait]
impl Endpoint for __quench_route_handle_head {
    async fn call(&self, _req: Request) -> Response {
        respond()
    }
}

quench_http::submit!(
    quench_http::route::ROUTES,
    quench_http::route::RouteRegistration,
    quench_http::route::RouteRegistration {
        method: Method::HEAD,
        pattern: "/v2/",
        endpoint: || std::sync::Arc::new(__quench_route_handle_head)
            as std::sync::Arc<dyn Endpoint>,
    }
);

fn respond() -> Response {
    Response::new(StatusCode::OK).header("docker-distribution-api-version", "registry/2.0")
}

pub fn register_routes() {
    let _ = handle_get as fn() -> _;
}
