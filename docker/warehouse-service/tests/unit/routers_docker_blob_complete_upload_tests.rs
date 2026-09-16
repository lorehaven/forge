use crate::support;

use http::{Method, StatusCode};
use warehouse_service::utils::sha256::sha256_hex;

fn start_upload(storage: &support::WithDockerStorageRoot, uuid: &str, content: &[u8]) {
    let upload_dir = storage.dir.path().join("my-repo").join("_uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();
    std::fs::write(upload_dir.join(uuid), content).unwrap();
}

fn blob_path(storage: &support::WithDockerStorageRoot, digest: &str) -> std::path::PathBuf {
    let hex = digest.strip_prefix("sha256:").unwrap();
    storage.dir.path().join("blobs").join("sha256").join(hex)
}

fn digest_of(content: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(content))
}

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::docker::blob::complete_upload::register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

fn put(
    path: &str,
    body: &[u8],
    container: &std::sync::Arc<quench_http::di::Container>,
) -> quench_http::request::Request {
    support::raw_req(Method::PUT, path, &[], body, container)
}

#[tokio::test]
async fn handle_rejects_a_malformed_digest() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(put(
            "/v2/my-repo/blobs/uploads/upload-1?digest=not-a-digest",
            b"",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handle_reports_not_found_for_an_unknown_upload() {
    let storage = support::WithDockerStorageRoot::new();
    let digest = digest_of(b"hello");
    let (app, container) = app().await;
    let resp = app
        .call(put(
            &format!("/v2/my-repo/blobs/uploads/no-such-upload?digest={digest}"),
            b"",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let _ = &storage;
}

#[tokio::test]
async fn handle_rejects_a_digest_mismatch() {
    let storage = support::WithDockerStorageRoot::new();
    start_upload(&storage, "upload-1", b"hello");
    let wrong_digest = digest_of(b"not hello");

    let (app, container) = app().await;
    let resp = app
        .call(put(
            &format!("/v2/my-repo/blobs/uploads/upload-1?digest={wrong_digest}"),
            b"",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handle_moves_the_upload_to_its_final_content_addressed_path() {
    let storage = support::WithDockerStorageRoot::new();
    start_upload(&storage, "upload-1", b"hello");
    let digest = digest_of(b"hello");

    let (app, container) = app().await;
    let resp = app
        .call(put(
            &format!("/v2/my-repo/blobs/uploads/upload-1?digest={digest}"),
            b"",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let (headers, _) = support::parts(resp).await;
    assert_eq!(
        headers.get("docker-content-digest").unwrap(),
        digest.as_str()
    );

    let final_path = blob_path(&storage, &digest);
    assert!(final_path.exists());
    assert_eq!(std::fs::read(&final_path).unwrap(), b"hello");

    let upload_file = storage
        .dir
        .path()
        .join("my-repo")
        .join("_uploads")
        .join("upload-1");
    assert!(!upload_file.exists());
}

#[tokio::test]
async fn handle_appends_a_final_chunk_from_the_request_body_before_verifying() {
    let storage = support::WithDockerStorageRoot::new();
    start_upload(&storage, "upload-1", b"hello ");
    let digest = digest_of(b"hello world");

    let (app, container) = app().await;
    let resp = app
        .call(put(
            &format!("/v2/my-repo/blobs/uploads/upload-1?digest={digest}"),
            b"world",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    assert_eq!(
        std::fs::read(blob_path(&storage, &digest)).unwrap(),
        b"hello world"
    );
}

#[tokio::test]
async fn handle_dedupes_when_the_blob_already_exists_at_the_final_path() {
    let storage = support::WithDockerStorageRoot::new();
    start_upload(&storage, "upload-1", b"hello");
    let digest = digest_of(b"hello");

    // Pre-seed the final path, as if another upload already completed it.
    let final_path = blob_path(&storage, &digest);
    std::fs::create_dir_all(final_path.parent().unwrap()).unwrap();
    std::fs::write(&final_path, b"hello").unwrap();

    let (app, container) = app().await;
    let resp = app
        .call(put(
            &format!("/v2/my-repo/blobs/uploads/upload-1?digest={digest}"),
            b"",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let upload_file = storage
        .dir
        .path()
        .join("my-repo")
        .join("_uploads")
        .join("upload-1");
    assert!(!upload_file.exists());
}

#[tokio::test]
async fn handle_streams_a_large_monolithic_put_without_buffering_the_whole_blob() {
    let storage = support::WithDockerStorageRoot::new();
    start_upload(&storage, "upload-1", b"");

    // Larger than the 64 KiB streaming buffer, so this exercises the multi-frame
    // append + off-disk hash path rather than a single read.
    let blob: Vec<u8> = (0..(256 * 1024 + 7)).map(|i| (i % 251) as u8).collect();
    let digest = digest_of(&blob);

    let (app, container) = app().await;
    let resp = app
        .call(put(
            &format!("/v2/my-repo/blobs/uploads/upload-1?digest={digest}"),
            &blob,
            &container,
        ))
        .await;

    assert_eq!(resp.status(), StatusCode::CREATED);
    assert_eq!(std::fs::read(blob_path(&storage, &digest)).unwrap(), blob);
}
