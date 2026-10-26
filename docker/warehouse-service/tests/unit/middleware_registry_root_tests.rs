use quench_core::async_trait;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::prelude::wrap;
use quench_http::request::Request;
use quench_http::response::Response;
use std::sync::Arc;
use warehouse_service::middleware::registry_root::{RegistryRootOnly, is_registry_under_base};

fn plain_req(path: &str) -> Request {
    let container = Arc::new(quench_http::di::Container::default());
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(quench_core::bytes::Bytes::new()),
        container,
    )
}

struct Ok200;

#[async_trait]
impl Endpoint for Ok200 {
    async fn call(&self, _req: Request) -> Response {
        Response::new(StatusCode::OK)
    }
}

fn app(base_path: &str) -> Arc<dyn Endpoint> {
    wrap(Arc::new(Ok200), RegistryRootOnly::new(base_path))
}

#[quench_test::test]
fn registry_paths_under_the_base_path_are_detected() {
    for path in [
        "/warehouse/v2",
        "/warehouse/v2/",
        "/warehouse/v2/_catalog",
        "/warehouse/v2/forge/x/blobs/uploads/",
        "/warehouse//v2/_catalog",
        "/warehouse/token",
    ] {
        assert!(is_registry_under_base("/warehouse", path), "{path}");
    }
}

#[quench_test::test]
fn other_paths_are_not_detected() {
    for path in [
        "/warehouse/ui",
        "/warehouse/api/v1/crates",
        "/v2/_catalog",
        "/token",
    ] {
        assert!(!is_registry_under_base("/warehouse", path), "{path}");
    }
}

#[quench_test::test]
fn nothing_is_detected_without_a_base_path() {
    assert!(!is_registry_under_base("", "/v2/_catalog"));
}

#[quench_test::test]
async fn registry_routes_under_the_base_path_are_not_found() {
    let app = app("/warehouse");
    for path in [
        "/warehouse/v2/_catalog",
        "/warehouse/v2/",
        "/warehouse/token",
    ] {
        assert_eq!(
            app.call(plain_req(path)).await.status(),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
}

#[quench_test::test]
async fn ordinary_routes_pass_through() {
    let app = app("/warehouse");
    for path in ["/warehouse/ui/home", "/warehouse/health"] {
        assert_eq!(
            app.call(plain_req(path)).await.status(),
            StatusCode::OK,
            "{path}"
        );
    }
}

#[quench_test::test]
async fn a_root_base_path_blocks_nothing() {
    let app = app("/");
    assert_eq!(
        app.call(plain_req("/v2/_catalog")).await.status(),
        StatusCode::OK
    );
}
