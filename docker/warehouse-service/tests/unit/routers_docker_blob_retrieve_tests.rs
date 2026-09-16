use crate::support;

use http::{Method, StatusCode};
use warehouse_service::routers::docker::blob::retrieve::{maybe_redirect, parse_range};

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn write_blob(storage: &support::WithDockerStorageRoot, content: &[u8]) {
    let hex = DIGEST.strip_prefix("sha256:").unwrap();
    let blob_dir = storage.dir.path().join("blobs").join("sha256");
    std::fs::create_dir_all(&blob_dir).unwrap();
    std::fs::write(blob_dir.join(hex), content).unwrap();
}

// -----------------------------------------------------------------
// parse_range
// -----------------------------------------------------------------

#[test]
fn parse_range_reads_a_fully_specified_range() {
    assert_eq!(parse_range("bytes=0-99", 200), Some((0, 99)));
}

#[test]
fn parse_range_defaults_the_end_to_the_last_byte_when_omitted() {
    assert_eq!(parse_range("bytes=50-", 200), Some((50, 199)));
}

#[test]
fn parse_range_rejects_a_missing_bytes_prefix() {
    assert_eq!(parse_range("0-99", 200), None);
}

#[test]
fn parse_range_rejects_malformed_numbers() {
    assert_eq!(parse_range("bytes=a-b", 200), None);
    assert_eq!(parse_range("bytes=0-99-200", 200), None);
}

#[test]
fn parse_range_rejects_a_start_past_the_end() {
    assert_eq!(parse_range("bytes=100-50", 200), None);
}

#[test]
fn parse_range_rejects_an_end_at_or_past_the_total_size() {
    assert_eq!(parse_range("bytes=0-200", 200), None);
    assert_eq!(parse_range("bytes=0-199", 200), Some((0, 199)));
}

// -----------------------------------------------------------------
// maybe_redirect
// -----------------------------------------------------------------

#[test]
fn maybe_redirect_is_none_when_the_flag_is_unset_or_false() {
    let _guard = support::redirect_env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::remove("ENABLE_REDIRECT");
    assert!(maybe_redirect(DIGEST).is_none());
}

#[test]
fn maybe_redirect_points_at_the_configured_backend_when_enabled() {
    let _guard = support::redirect_env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::set("ENABLE_REDIRECT", "true");
    envmnt::set("BLOB_REDIRECT_BASE", "https://cdn.example.com");
    let resp = maybe_redirect(DIGEST).expect("redirect");
    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = resp
        .into_hyper()
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        location,
        "https://cdn.example.com/blobs/sha256/e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    envmnt::remove("ENABLE_REDIRECT");
    envmnt::remove("BLOB_REDIRECT_BASE");
}

// -----------------------------------------------------------------
// handle
// -----------------------------------------------------------------

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::docker::blob::retrieve::register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

fn get_with_range(
    path: &str,
    range: Option<&str>,
    container: &std::sync::Arc<quench_http::di::Container>,
) -> quench_http::request::Request {
    match range {
        Some(r) => support::raw_req(Method::GET, path, &[("range", r)], b"", container),
        None => support::req(Method::GET, path, container),
    }
}

#[tokio::test]
async fn handle_rejects_a_malformed_digest() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
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
            Method::GET,
            &format!("/v2/my-repo/blobs/{DIGEST}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn handle_serves_the_full_blob_without_a_range_header() {
    let storage = support::WithDockerStorageRoot::new();
    write_blob(&storage, b"hello world");

    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/v2/my-repo/blobs/{DIGEST}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = support::body_bytes(resp).await;
    assert_eq!(&body[..], b"hello world");
}

#[tokio::test]
async fn handle_serves_a_partial_range_when_requested() {
    let storage = support::WithDockerStorageRoot::new();
    write_blob(&storage, b"hello world");

    let (app, container) = app().await;
    let resp = app
        .call(get_with_range(
            &format!("/v2/my-repo/blobs/{DIGEST}"),
            Some("bytes=0-4"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    let body = support::body_bytes(resp).await;
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn handle_reports_range_not_satisfiable_for_a_bogus_range() {
    let storage = support::WithDockerStorageRoot::new();
    write_blob(&storage, b"hello world");

    let (app, container) = app().await;
    let resp = app
        .call(get_with_range(
            &format!("/v2/my-repo/blobs/{DIGEST}"),
            Some("bytes=500-600"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
}

#[tokio::test]
async fn handle_streams_a_large_full_blob_across_many_frames() {
    let storage = support::WithDockerStorageRoot::new();
    // Well past ReaderStream's frame size, so the body is delivered in pieces.
    let blob: Vec<u8> = (0..(512 * 1024 + 3)).map(|i| (i % 253) as u8).collect();
    write_blob(&storage, &blob);

    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/v2/my-repo/blobs/{DIGEST}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    // Binary content, so read headers+body from one `into_hyper()` call rather
    // than `support::parts` (which lossily re-decodes the body as UTF-8 text).
    use http_body_util::BodyExt;
    let (parts, body) = resp.into_hyper().into_parts();
    assert_eq!(
        parts.headers.get("content-length").unwrap(),
        blob.len().to_string().as_str()
    );
    let body = body.collect().await.expect("body").to_bytes();
    assert_eq!(&body[..], &blob[..]);
}

#[tokio::test]
async fn handle_streams_a_partial_range_out_of_a_large_blob() {
    let storage = support::WithDockerStorageRoot::new();
    let blob: Vec<u8> = (0..(512 * 1024)).map(|i| (i % 253) as u8).collect();
    write_blob(&storage, &blob);

    let (app, container) = app().await;
    let resp = app
        .call(get_with_range(
            &format!("/v2/my-repo/blobs/{DIGEST}"),
            Some("bytes=100000-359999"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    let body = support::body_bytes(resp).await;
    assert_eq!(&body[..], &blob[100_000..=359_999]);
}
