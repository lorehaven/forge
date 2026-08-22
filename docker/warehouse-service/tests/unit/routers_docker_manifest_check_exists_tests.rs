use crate::support;

use actix_web::test as actix_test;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::manifest::check_exists::handle;
use warehouse_service::utils::sha256::sha256_hex;

fn write_manifest_and_tag(storage: &WithStorageRoot, repo: &str, tag: &str, manifest: &[u8]) {
    let hex = sha256_hex(manifest);
    let digest = format!("sha256:{hex}");

    let manifests_dir = storage.dir.path().join("manifests").join("sha256");
    std::fs::create_dir_all(&manifests_dir).unwrap();
    std::fs::write(manifests_dir.join(&hex), manifest).unwrap();

    let tags_dir = storage.dir.path().join(repo).join("tags");
    std::fs::create_dir_all(&tags_dir).unwrap();
    std::fs::write(tags_dir.join(tag), &digest).unwrap();
}

const MANIFEST_JSON: &str = r#"{"schemaVersion": 2, "mediaType": "application/vnd.docker.distribution.manifest.v2+json", "config": {}, "layers": []}"#;

#[actix_web::test]
async fn handle_reports_not_found_for_a_missing_tag() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/my-repo/manifests/latest")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn handle_reports_ok_with_the_digest_for_an_existing_tag() {
    let storage = WithStorageRoot::new();
    write_manifest_and_tag(&storage, "my-repo", "latest", MANIFEST_JSON.as_bytes());

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/my-repo/manifests/latest")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert!(resp.headers().contains_key("Docker-Content-Digest"));
}
