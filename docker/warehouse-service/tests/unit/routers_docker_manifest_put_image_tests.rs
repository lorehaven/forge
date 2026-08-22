use crate::support;

use actix_web::test as actix_test;
use serde_json::Value;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::manifest::put_image::{
    DOCKER_MANIFEST_LIST_V2, DOCKER_MANIFEST_V2, DescriptorKind, OCI_IMAGE_INDEX_V1,
    OCI_IMAGE_MANIFEST_V1, handle, is_supported_manifest_media_type, normalize_manifest_body,
    normalize_media_type, populate_descriptor_size,
};
use warehouse_service::utils::sha256::sha256_hex;

// -----------------------------------------------------------------
// normalize_media_type / is_supported_manifest_media_type
// -----------------------------------------------------------------

#[test]
fn normalize_media_type_strips_parameters_and_trims() {
    assert_eq!(
        normalize_media_type(" application/json ; charset=utf-8"),
        Some("application/json")
    );
}

#[test]
fn normalize_media_type_is_none_for_an_empty_value() {
    assert_eq!(normalize_media_type("  ;charset=utf-8"), None);
}

#[test]
fn is_supported_manifest_media_type_accepts_every_known_type_and_rejects_others() {
    for mt in [
        DOCKER_MANIFEST_V2,
        DOCKER_MANIFEST_LIST_V2,
        OCI_IMAGE_MANIFEST_V1,
        OCI_IMAGE_INDEX_V1,
    ] {
        assert!(is_supported_manifest_media_type(mt));
    }
    assert!(!is_supported_manifest_media_type("application/json"));
}

// -----------------------------------------------------------------
// normalize_manifest_body / populate_descriptor_size
// -----------------------------------------------------------------

#[tokio::test]
async fn normalize_manifest_body_rejects_invalid_json() {
    assert!(normalize_manifest_body(b"not json").await.is_err());
}

#[tokio::test]
async fn normalize_manifest_body_rejects_a_schema_version_other_than_2() {
    assert!(
        normalize_manifest_body(br#"{"schemaVersion": 1}"#)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn normalize_manifest_body_leaves_a_manifest_with_no_descriptors_untouched() {
    let body = normalize_manifest_body(br#"{"schemaVersion": 2}"#)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["schemaVersion"], 2);
}

#[tokio::test]
async fn normalize_manifest_body_keeps_an_explicit_nonzero_size_as_is() {
    let json = r#"{"schemaVersion": 2, "config": {"digest": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "size": 42}}"#;
    let body = normalize_manifest_body(json.as_bytes()).await.unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["config"]["size"], 42);
}

#[tokio::test]
async fn populate_descriptor_size_rejects_a_descriptor_that_is_not_an_object() {
    let mut descriptor = Value::String("not an object".to_string());
    let err = populate_descriptor_size(&mut descriptor, DescriptorKind::Blob)
        .await
        .unwrap_err();
    assert_eq!(err, "invalid manifest descriptor");
}

#[tokio::test]
async fn populate_descriptor_size_requires_a_digest_field() {
    let mut descriptor = serde_json::json!({});
    let err = populate_descriptor_size(&mut descriptor, DescriptorKind::Blob)
        .await
        .unwrap_err();
    assert_eq!(err, "manifest descriptor is missing digest");
}

#[tokio::test]
async fn populate_descriptor_size_rejects_a_malformed_digest() {
    let mut descriptor = serde_json::json!({"digest": "not-a-digest"});
    let err = populate_descriptor_size(&mut descriptor, DescriptorKind::Blob)
        .await
        .unwrap_err();
    assert_eq!(err, "manifest descriptor has invalid digest");
}

// -----------------------------------------------------------------
// handle
// -----------------------------------------------------------------

fn write_blob(storage: &WithStorageRoot, content: &[u8]) -> String {
    let hex = sha256_hex(content);
    let dir = storage.dir.path().join("blobs").join("sha256");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(&hex), content).unwrap();
    format!("sha256:{hex}")
}

#[actix_web::test]
async fn handle_rejects_an_unsupported_content_type() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri("/my-repo/manifests/latest")
        .insert_header(("Content-Type", "text/plain"))
        .set_payload(br#"{"schemaVersion": 2}"#.to_vec())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
}

#[actix_web::test]
async fn handle_rejects_an_invalid_manifest_body() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri("/my-repo/manifests/latest")
        .set_payload(b"not json".to_vec())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_stores_the_manifest_by_digest_and_writes_the_tag() {
    let storage = WithStorageRoot::new();
    let body = br#"{"schemaVersion": 2}"#;
    // The handler re-serializes the manifest via `normalize_manifest_body`
    // before hashing it (so descriptor `size` fields can be filled in),
    // so the stored digest is over that normalized form, not the raw
    // request body byte-for-byte - compute it the same way.
    let normalized = normalize_manifest_body(body).await.unwrap();
    let expected_digest = format!("sha256:{}", sha256_hex(&normalized));

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri("/my-repo/manifests/latest")
        .set_payload(body.to_vec())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);
    assert_eq!(
        resp.headers().get("Docker-Content-Digest").unwrap(),
        expected_digest.as_str()
    );

    let hex = expected_digest.strip_prefix("sha256:").unwrap();
    let manifest_path = storage
        .dir
        .path()
        .join("manifests")
        .join("sha256")
        .join(hex);
    assert!(manifest_path.exists());

    let tag_path = storage
        .dir
        .path()
        .join("my-repo")
        .join("tags")
        .join("latest");
    assert_eq!(std::fs::read_to_string(&tag_path).unwrap(), expected_digest);
}

#[actix_web::test]
async fn handle_does_not_write_a_tag_when_the_reference_is_already_a_digest() {
    let storage = WithStorageRoot::new();
    let body = br#"{"schemaVersion": 2}"#;
    let digest = format!("sha256:{}", sha256_hex(body));

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri(&format!("/my-repo/manifests/{digest}"))
        .set_payload(body.to_vec())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);

    let tags_dir = storage.dir.path().join("my-repo").join("tags");
    assert!(!tags_dir.exists() || std::fs::read_dir(&tags_dir).unwrap().count() == 0);
}

#[actix_web::test]
async fn handle_fills_in_descriptor_sizes_from_blobs_already_on_disk() {
    let storage = WithStorageRoot::new();
    let blob_digest = write_blob(&storage, b"layer content");
    let body = format!(r#"{{"schemaVersion": 2, "config": {{"digest": "{blob_digest}"}}}}"#);

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri("/my-repo/manifests/latest")
        .set_payload(body.into_bytes())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);

    let digest_header = resp
        .headers()
        .get("Docker-Content-Digest")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let hex = digest_header.strip_prefix("sha256:").unwrap();
    let stored = std::fs::read(
        storage
            .dir
            .path()
            .join("manifests")
            .join("sha256")
            .join(hex),
    )
    .unwrap();
    let value: Value = serde_json::from_slice(&stored).unwrap();
    assert_eq!(value["config"]["size"], "layer content".len());
}

#[actix_web::test]
async fn handle_rejects_a_manifest_referencing_a_blob_that_does_not_exist() {
    let _storage = WithStorageRoot::new();
    let missing_digest = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let body = format!(r#"{{"schemaVersion": 2, "config": {{"digest": "{missing_digest}"}}}}"#);

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::put()
        .uri("/my-repo/manifests/latest")
        .set_payload(body.into_bytes())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}
