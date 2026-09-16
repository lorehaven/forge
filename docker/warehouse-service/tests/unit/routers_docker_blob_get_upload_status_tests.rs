use crate::support;

use http::{Method, StatusCode};

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::docker::blob::get_upload_status::register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

#[tokio::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/v2/..%2fetc/blobs/uploads/some-uuid",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handle_reports_not_found_for_an_unknown_upload() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/v2/my-repo/blobs/uploads/no-such-upload",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn handle_reports_a_zero_range_for_an_empty_upload() {
    let storage = support::WithDockerStorageRoot::new();
    let upload_dir = storage.dir.path().join("my-repo").join("_uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();
    std::fs::write(upload_dir.join("upload-1"), b"").unwrap();

    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/v2/my-repo/blobs/uploads/upload-1",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let (headers, _) = support::parts(resp).await;
    assert_eq!(headers.get("range").unwrap(), "0-0");
}

#[tokio::test]
async fn handle_reports_the_written_range_for_a_partial_upload() {
    let storage = support::WithDockerStorageRoot::new();
    let upload_dir = storage.dir.path().join("my-repo").join("_uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();
    std::fs::write(upload_dir.join("upload-1"), b"12345").unwrap();

    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/v2/my-repo/blobs/uploads/upload-1",
            &container,
        ))
        .await;
    let (headers, _) = support::parts(resp).await;
    assert_eq!(headers.get("range").unwrap(), "0-4");
    assert_eq!(headers.get("docker-upload-uuid").unwrap(), "upload-1");
}
