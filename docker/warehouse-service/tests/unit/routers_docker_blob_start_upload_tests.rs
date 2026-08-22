use crate::support;

use actix_web::test as actix_test;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::blob::start_upload::handle;

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[actix_web::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::post()
        .uri("/..%2fetc/blobs/uploads/")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_starts_a_regular_upload_and_creates_the_staging_file() {
    let storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::post()
        .uri("/my-repo/blobs/uploads/")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::ACCEPTED);
    assert_eq!(resp.headers().get("Range").unwrap(), "0-0");
    assert!(resp.headers().contains_key("Docker-Upload-UUID"));

    let uploads_dir = storage.dir.path().join("my-repo").join("_uploads");
    assert!(uploads_dir.exists());
    assert_eq!(std::fs::read_dir(&uploads_dir).unwrap().count(), 1);
}

#[actix_web::test]
async fn handle_rejects_a_cross_repo_mount_with_an_invalid_digest() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::post()
        .uri("/my-repo/blobs/uploads/?mount=not-a-digest&from=other-repo")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_mounts_an_existing_blob_from_another_repository() {
    let storage = WithStorageRoot::new();
    let hex = DIGEST.strip_prefix("sha256:").unwrap();
    let blob_dir = storage.dir.path().join("blobs").join("sha256");
    std::fs::create_dir_all(&blob_dir).unwrap();
    std::fs::write(blob_dir.join(hex), b"content").unwrap();

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::post()
        .uri(&format!(
            "/my-repo/blobs/uploads/?mount={DIGEST}&from=other-repo"
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);
    assert_eq!(resp.headers().get("Docker-Content-Digest").unwrap(), DIGEST);
}

#[actix_web::test]
async fn handle_falls_back_to_a_regular_upload_when_the_mount_target_does_not_exist() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::post()
        .uri(&format!(
            "/my-repo/blobs/uploads/?mount={DIGEST}&from=other-repo"
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::ACCEPTED);
}
