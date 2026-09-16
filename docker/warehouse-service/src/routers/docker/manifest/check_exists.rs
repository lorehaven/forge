use super::AcceptHeader;
use super::get_image::resolve_manifest_response;
use quench_http::prelude::{Endpoint, FromRequest, Path, Request, Response, http::Method};

pub async fn handle(
    Path((name, reference)): Path<(String, String)>,
    accept: AcceptHeader,
) -> Response {
    let resolved =
        match resolve_manifest_response(accept.0.as_deref().unwrap_or(""), &name, &reference).await
        {
            Ok(v) => v,
            Err(resp) => return resp,
        };

    Response::new(quench_http::prelude::http::StatusCode::OK)
        .header("content-type", resolved.media_type)
        .header("docker-content-digest", &resolved.digest)
}

// No `#[head]` macro exists - hand-expanded from `route_impl` in quench-http-macros.
#[allow(non_camel_case_types)]
struct __quench_route_handle_head;

#[async_trait::async_trait]
impl Endpoint for __quench_route_handle_head {
    async fn call(&self, mut req: Request) -> Response {
        let arg0 = match <Path<(String, String)> as FromRequest>::from_request(&mut req).await {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        let arg1 = match <AcceptHeader as FromRequest>::from_request(&mut req).await {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        handle(arg0, arg1).await
    }
}

quench_http::inventory::submit! {
    quench_http::prelude::RouteRegistration {
        method: Method::HEAD,
        pattern: "/v2/{name:.+}/manifests/{reference}",
        endpoint: || std::sync::Arc::new(__quench_route_handle_head) as std::sync::Arc<dyn Endpoint>,
    }
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
