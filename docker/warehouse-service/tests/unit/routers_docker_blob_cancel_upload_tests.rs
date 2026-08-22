use crate::support;

use actix_web::test as actix_test;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::blob::cancel_upload::handle;

#[actix_web::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::delete()
        .uri("/..%2fetc/blobs/uploads/some-uuid")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_reports_not_found_for_an_unknown_upload() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::delete()
        .uri("/my-repo/blobs/uploads/no-such-upload")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn handle_deletes_an_existing_upload() {
    let storage = WithStorageRoot::new();
    let upload_dir = storage.dir.path().join("my-repo").join("_uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();
    let upload_file = upload_dir.join("upload-1");
    std::fs::write(&upload_file, b"partial").unwrap();

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::delete()
        .uri("/my-repo/blobs/uploads/upload-1")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);
    assert!(!upload_file.exists());
}
