use crate::support;

use actix_web::test as actix_test;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::blob::complete_upload::handle;
use warehouse_service::utils::sha256::sha256_hex;

fn start_upload(storage: &WithStorageRoot, uuid: &str, content: &[u8]) {
    let upload_dir = storage.dir.path().join("my-repo").join("_uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();
    std::fs::write(upload_dir.join(uuid), content).unwrap();
}

fn blob_path(storage: &WithStorageRoot, digest: &str) -> std::path::PathBuf {
    let hex = digest.strip_prefix("sha256:").unwrap();
    storage.dir.path().join("blobs").join("sha256").join(hex)
}

fn digest_of(content: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(content))
}

#[actix_web::test]
async fn handle_rejects_a_malformed_digest() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri("/my-repo/blobs/uploads/upload-1?digest=not-a-digest")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_reports_not_found_for_an_unknown_upload() {
    let storage = WithStorageRoot::new();
    let digest = digest_of(b"hello");
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri(&format!(
            "/my-repo/blobs/uploads/no-such-upload?digest={digest}"
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
    let _ = &storage;
}

#[actix_web::test]
async fn handle_rejects_a_digest_mismatch() {
    let storage = WithStorageRoot::new();
    start_upload(&storage, "upload-1", b"hello");
    let wrong_digest = digest_of(b"not hello");

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri(&format!(
            "/my-repo/blobs/uploads/upload-1?digest={wrong_digest}"
        ))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_moves_the_upload_to_its_final_content_addressed_path() {
    let storage = WithStorageRoot::new();
    start_upload(&storage, "upload-1", b"hello");
    let digest = digest_of(b"hello");

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri(&format!("/my-repo/blobs/uploads/upload-1?digest={digest}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);
    assert_eq!(
        resp.headers().get("Docker-Content-Digest").unwrap(),
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

#[actix_web::test]
async fn handle_appends_a_final_chunk_from_the_request_body_before_verifying() {
    let storage = WithStorageRoot::new();
    start_upload(&storage, "upload-1", b"hello ");
    let digest = digest_of(b"hello world");

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri(&format!("/my-repo/blobs/uploads/upload-1?digest={digest}"))
        .set_payload(b"world".to_vec())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);
    assert_eq!(
        std::fs::read(blob_path(&storage, &digest)).unwrap(),
        b"hello world"
    );
}

#[actix_web::test]
async fn handle_dedupes_when_the_blob_already_exists_at_the_final_path() {
    let storage = WithStorageRoot::new();
    start_upload(&storage, "upload-1", b"hello");
    let digest = digest_of(b"hello");

    // Pre-seed the final path, as if another upload already completed it.
    let final_path = blob_path(&storage, &digest);
    std::fs::create_dir_all(final_path.parent().unwrap()).unwrap();
    std::fs::write(&final_path, b"hello").unwrap();

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri(&format!("/my-repo/blobs/uploads/upload-1?digest={digest}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);

    let upload_file = storage
        .dir
        .path()
        .join("my-repo")
        .join("_uploads")
        .join("upload-1");
    assert!(!upload_file.exists());
}

#[actix_web::test]
async fn handle_streams_a_large_monolithic_put_without_buffering_the_whole_blob() {
    let storage = WithStorageRoot::new();
    start_upload(&storage, "upload-1", b"");

    // Larger than the 64 KiB streaming buffer, so this exercises the multi-frame
    // append + off-disk hash path rather than a single read.
    let blob: Vec<u8> = (0..(256 * 1024 + 7)).map(|i| (i % 251) as u8).collect();
    let digest = digest_of(&blob);

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri(&format!("/my-repo/blobs/uploads/upload-1?digest={digest}"))
        .set_payload(blob.clone())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);
    assert_eq!(std::fs::read(blob_path(&storage, &digest)).unwrap(), blob);
}
