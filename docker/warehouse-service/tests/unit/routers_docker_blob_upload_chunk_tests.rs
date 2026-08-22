use crate::support;

use actix_web::test as actix_test;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::blob::upload_chunk::handle;

fn start_upload(storage: &WithStorageRoot, uuid: &str, initial: &[u8]) -> std::path::PathBuf {
    let upload_dir = storage.dir.path().join("my-repo").join("_uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();
    let path = upload_dir.join(uuid);
    std::fs::write(&path, initial).unwrap();
    path
}

#[actix_web::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::patch()
        .uri("/..%2fetc/blobs/uploads/some-uuid")
        .set_payload(b"chunk".to_vec())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_reports_not_found_for_an_unknown_upload() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::patch()
        .uri("/my-repo/blobs/uploads/no-such-upload")
        .set_payload(b"chunk".to_vec())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn handle_rejects_an_empty_chunk() {
    let storage = WithStorageRoot::new();
    start_upload(&storage, "upload-1", b"");

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::patch()
        .uri("/my-repo/blobs/uploads/upload-1")
        .set_payload(Vec::<u8>::new())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_appends_a_chunk_and_reports_the_new_range() {
    let storage = WithStorageRoot::new();
    let path = start_upload(&storage, "upload-1", b"hello");

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::patch()
        .uri("/my-repo/blobs/uploads/upload-1")
        .set_payload(b" world".to_vec())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::ACCEPTED);
    assert_eq!(resp.headers().get("Range").unwrap(), "0-10");

    let content = std::fs::read(&path).unwrap();
    assert_eq!(content, b"hello world");
}
