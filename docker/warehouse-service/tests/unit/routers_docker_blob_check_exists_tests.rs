use crate::support;

use actix_web::test as actix_test;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::blob::check_exists::handle;

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[actix_web::test]
async fn handle_rejects_a_malformed_digest() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/my-repo/blobs/not-a-digest")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_reports_not_found_for_a_missing_blob() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri(&format!("/my-repo/blobs/{DIGEST}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn handle_reports_the_blob_s_length_and_digest_when_present() {
    let storage = WithStorageRoot::new();
    let hex = DIGEST.strip_prefix("sha256:").unwrap();
    let blob_dir = storage.dir.path().join("blobs").join("sha256");
    std::fs::create_dir_all(&blob_dir).unwrap();
    std::fs::write(blob_dir.join(hex), b"hello world").unwrap();

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri(&format!("/my-repo/blobs/{DIGEST}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert_eq!(resp.headers().get("Content-Length").unwrap(), "11");
    assert_eq!(resp.headers().get("Docker-Content-Digest").unwrap(), DIGEST);
}
