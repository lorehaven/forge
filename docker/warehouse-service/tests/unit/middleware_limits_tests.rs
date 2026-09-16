use async_trait::async_trait;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::endpoint::Endpoint;
use quench_http::prelude::wrap;
use quench_http::request::Request;
use quench_http::response::Response;
use std::sync::Arc;
use warehouse_service::middleware::limits::{WarehouseLimits, is_upload_mutation};

fn plain_req(method: Method, path: &str) -> Request {
    let container = Arc::new(quench_http::di::Container::default());
    Request::new(
        method,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(bytes::Bytes::new()),
        container,
    )
}

#[test]
fn is_upload_mutation_true_for_write_methods_on_the_uploads_path() {
    for method in [Method::POST, Method::PATCH, Method::PUT] {
        let req = plain_req(method, "/v2/my/repo/blobs/uploads/some-uuid");
        assert!(is_upload_mutation(&req));
    }
}

#[test]
fn is_upload_mutation_false_for_reads_even_on_the_uploads_path() {
    let req = plain_req(Method::GET, "/v2/my/repo/blobs/uploads/some-uuid");
    assert!(!is_upload_mutation(&req));
}

#[test]
fn is_upload_mutation_false_for_writes_elsewhere() {
    let req = plain_req(Method::POST, "/v2/my/repo/manifests/latest");
    assert!(!is_upload_mutation(&req));
}

/// The middleware's "next" - a fixed 200 after a short sleep, mirroring the
/// old actix test app's handler: a real request holds its permit for the
/// length of the handler, and the sleep gives a second concurrent request
/// time to observe the semaphore as exhausted before this one releases it.
struct SlowOk;

#[async_trait]
impl Endpoint for SlowOk {
    async fn call(&self, _req: Request) -> Response {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Response::new(StatusCode::OK)
    }
}

fn test_app(permits: usize) -> Arc<dyn Endpoint> {
    wrap(Arc::new(SlowOk), WarehouseLimits::new(permits))
}

#[tokio::test]
async fn non_upload_requests_bypass_the_limiter_entirely() {
    let app = test_app(0);
    let req = plain_req(Method::GET, "/v2/my/repo/manifests/latest");
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_second_concurrent_upload_beyond_the_limit_is_throttled() {
    let app = test_app(1);
    let app_for_first = app.clone();

    let first = tokio::spawn(async move {
        let req = plain_req(Method::POST, "/v2/my/repo/blobs/uploads/one");
        app_for_first.call(req).await.status()
    });
    // Give the first request time to acquire its permit before the
    // second one tries.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let req = plain_req(Method::POST, "/v2/my/repo/blobs/uploads/two");
    let second_status = app.call(req).await.status();
    let first_status = first.await.expect("first request task");

    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(second_status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn a_new_max_concurrent_uploads_of_zero_is_treated_as_at_least_one() {
    // `WarehouseLimits::new(0)` must not create a semaphore with zero
    // permits, which would make every upload request fail forever.
    let app = test_app(0);
    let req = plain_req(Method::POST, "/v2/my/repo/blobs/uploads/one");
    let resp = app.call(req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
