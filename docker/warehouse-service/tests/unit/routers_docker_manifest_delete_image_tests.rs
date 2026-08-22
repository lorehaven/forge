use crate::support;

use actix_web::test as actix_test;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::manifest::delete_image::handle;

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn write_manifest(storage: &WithStorageRoot) -> std::path::PathBuf {
    let hex = DIGEST.strip_prefix("sha256:").unwrap();
    let dir = storage.dir.path().join("manifests").join("sha256");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(hex);
    std::fs::write(&path, b"{}").unwrap();
    path
}

fn write_tag(storage: &WithStorageRoot, repo: &str, tag: &str, digest: &str) -> std::path::PathBuf {
    let dir = storage.dir.path().join(repo).join("tags");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(tag);
    std::fs::write(&path, digest).unwrap();
    path
}

#[actix_web::test]
async fn handle_rejects_a_non_digest_reference() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::delete()
        .uri("/my-repo/manifests/latest")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::METHOD_NOT_ALLOWED
    );
}

#[actix_web::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::delete()
        .uri(&format!("/..%2fetc/manifests/{DIGEST}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_reports_not_found_for_an_unknown_manifest() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::delete()
        .uri(&format!("/my-repo/manifests/{DIGEST}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn handle_deletes_the_manifest_and_every_tag_pointing_at_it() {
    let storage = WithStorageRoot::new();
    let manifest_path = write_manifest(&storage);
    let matching_tag = write_tag(&storage, "my-repo", "latest", DIGEST);
    let other_tag = write_tag(
        &storage,
        "my-repo",
        "v1",
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    );

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::delete()
        .uri(&format!("/my-repo/manifests/{DIGEST}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::ACCEPTED);

    assert!(!manifest_path.exists());
    assert!(
        !matching_tag.exists(),
        "tag pointing at the deleted digest should be removed"
    );
    assert!(other_tag.exists(), "unrelated tag should be left alone");
}
