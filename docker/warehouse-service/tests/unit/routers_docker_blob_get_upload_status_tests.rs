use crate::support;

use actix_web::test as actix_test;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::blob::get_upload_status::handle;

#[actix_web::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri("/..%2fetc/blobs/uploads/some-uuid")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_reports_not_found_for_an_unknown_upload() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri("/my-repo/blobs/uploads/no-such-upload")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn handle_reports_a_zero_range_for_an_empty_upload() {
    let storage = WithStorageRoot::new();
    let upload_dir = storage.dir.path().join("my-repo").join("_uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();
    std::fs::write(upload_dir.join("upload-1"), b"").unwrap();

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri("/my-repo/blobs/uploads/upload-1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);
    assert_eq!(resp.headers().get("Range").unwrap(), "0-0");
}

#[actix_web::test]
async fn handle_reports_the_written_range_for_a_partial_upload() {
    let storage = WithStorageRoot::new();
    let upload_dir = storage.dir.path().join("my-repo").join("_uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();
    std::fs::write(upload_dir.join("upload-1"), b"12345").unwrap();

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri("/my-repo/blobs/uploads/upload-1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.headers().get("Range").unwrap(), "0-4");
    assert_eq!(
        resp.headers().get("Docker-Upload-UUID").unwrap(),
        "upload-1"
    );
}
