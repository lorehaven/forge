use crate::support;

use http::{Method, StatusCode};

fn start_upload(
    storage: &support::WithDockerStorageRoot,
    uuid: &str,
    initial: &[u8],
) -> std::path::PathBuf {
    let upload_dir = storage.dir.path().join("my-repo").join("_uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();
    let path = upload_dir.join(uuid);
    std::fs::write(&path, initial).unwrap();
    path
}

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::docker::blob::upload_chunk::register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

fn patch(
    path: &str,
    body: &[u8],
    container: &std::sync::Arc<quench_http::di::Container>,
) -> quench_http::request::Request {
    support::raw_req(Method::PATCH, path, &[], body, container)
}

#[tokio::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(patch(
            "/v2/..%2fetc/blobs/uploads/some-uuid",
            b"chunk",
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
        .call(patch(
            "/v2/my-repo/blobs/uploads/no-such-upload",
            b"chunk",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn handle_rejects_an_empty_chunk() {
    let storage = support::WithDockerStorageRoot::new();
    start_upload(&storage, "upload-1", b"");

    let (app, container) = app().await;
    let resp = app
        .call(patch("/v2/my-repo/blobs/uploads/upload-1", b"", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handle_appends_a_chunk_and_reports_the_new_range() {
    let storage = support::WithDockerStorageRoot::new();
    let path = start_upload(&storage, "upload-1", b"hello");

    let (app, container) = app().await;
    let resp = app
        .call(patch(
            "/v2/my-repo/blobs/uploads/upload-1",
            b" world",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let (headers, _) = support::parts(resp).await;
    assert_eq!(headers.get("range").unwrap(), "0-10");

    let content = std::fs::read(&path).unwrap();
    assert_eq!(content, b"hello world");
}

#[tokio::test]
async fn handle_streams_a_large_chunk_onto_the_upload_file() {
    let storage = support::WithDockerStorageRoot::new();
    let path = start_upload(&storage, "upload-1", b"head:");

    // Bigger than the 64 KiB streaming buffer, so the append loop runs several
    // iterations rather than a single write.
    let chunk: Vec<u8> = (0..(200 * 1024)).map(|i| (i % 249) as u8).collect();

    let (app, container) = app().await;
    let resp = app
        .call(patch(
            "/v2/my-repo/blobs/uploads/upload-1",
            &chunk,
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let (headers, _) = support::parts(resp).await;
    assert_eq!(
        headers.get("range").unwrap(),
        format!("0-{}", 5 + chunk.len() - 1).as_str()
    );

    let mut expected = b"head:".to_vec();
    expected.extend_from_slice(&chunk);
    assert_eq!(std::fs::read(&path).unwrap(), expected);
}
