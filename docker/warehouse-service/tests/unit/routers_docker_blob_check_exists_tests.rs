use crate::support;

use http::{Method, StatusCode};

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::docker::blob::check_exists::register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

#[tokio::test]
async fn handle_rejects_a_malformed_digest() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::HEAD,
            "/v2/my-repo/blobs/not-a-digest",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handle_reports_not_found_for_a_missing_blob() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::HEAD,
            &format!("/v2/my-repo/blobs/{DIGEST}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn handle_reports_the_blob_s_length_and_digest_when_present() {
    let storage = support::WithDockerStorageRoot::new();
    let hex = DIGEST.strip_prefix("sha256:").unwrap();
    let blob_dir = storage.dir.path().join("blobs").join("sha256");
    std::fs::create_dir_all(&blob_dir).unwrap();
    std::fs::write(blob_dir.join(hex), b"hello world").unwrap();

    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::HEAD,
            &format!("/v2/my-repo/blobs/{DIGEST}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (headers, _) = support::parts(resp).await;
    assert_eq!(headers.get("content-length").unwrap(), "11");
    assert_eq!(headers.get("docker-content-digest").unwrap(), DIGEST);
}
