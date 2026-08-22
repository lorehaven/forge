use crate::support;

use actix_web::test as actix_test;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::blob::retrieve::{handle, maybe_redirect, parse_range};

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn write_blob(storage: &WithStorageRoot, content: &[u8]) {
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
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::TEMPORARY_REDIRECT
    );
    let location = resp.headers().get("Location").unwrap().to_str().unwrap();
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

#[actix_web::test]
async fn handle_rejects_a_malformed_digest() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri("/my-repo/blobs/not-a-digest")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_reports_not_found_for_a_missing_blob() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri(&format!("/my-repo/blobs/{DIGEST}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn handle_serves_the_full_blob_without_a_range_header() {
    let storage = WithStorageRoot::new();
    write_blob(&storage, b"hello world");

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri(&format!("/my-repo/blobs/{DIGEST}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body = actix_test::read_body(resp).await;
    assert_eq!(&body[..], b"hello world");
}

#[actix_web::test]
async fn handle_serves_a_partial_range_when_requested() {
    let storage = WithStorageRoot::new();
    write_blob(&storage, b"hello world");

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri(&format!("/my-repo/blobs/{DIGEST}"))
        .insert_header(("Range", "bytes=0-4"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::PARTIAL_CONTENT);
    let body = actix_test::read_body(resp).await;
    assert_eq!(&body[..], b"hello");
}

#[actix_web::test]
async fn handle_reports_range_not_satisfiable_for_a_bogus_range() {
    let storage = WithStorageRoot::new();
    write_blob(&storage, b"hello world");

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri(&format!("/my-repo/blobs/{DIGEST}"))
        .insert_header(("Range", "bytes=500-600"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::RANGE_NOT_SATISFIABLE
    );
}
