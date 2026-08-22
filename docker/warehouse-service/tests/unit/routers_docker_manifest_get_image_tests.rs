use crate::support;

use actix_web::test as actix_test;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::manifest::get_image::{
    DOCKER_MANIFEST_LIST_V2, DOCKER_MANIFEST_V2, OCI_IMAGE_INDEX_V1, OCI_IMAGE_MANIFEST_V1,
    detect_manifest_media_type, handle, media_match, negotiate_media_type, parse_accept,
};
use warehouse_service::utils::sha256::sha256_hex;

// -----------------------------------------------------------------
// detect_manifest_media_type
// -----------------------------------------------------------------

#[test]
fn detect_media_type_reads_the_explicit_mediatype_field() {
    assert_eq!(
        detect_manifest_media_type(
            format!(r#"{{"mediaType": "{DOCKER_MANIFEST_V2}"}}"#).as_bytes()
        ),
        Some(DOCKER_MANIFEST_V2)
    );
}

#[test]
fn detect_media_type_rejects_an_unrecognized_explicit_mediatype() {
    assert_eq!(
        detect_manifest_media_type(br#"{"mediaType": "application/x-nonsense"}"#),
        None
    );
}

#[test]
fn detect_media_type_rejects_non_schema_version_2() {
    assert_eq!(detect_manifest_media_type(br#"{"schemaVersion": 1}"#), None);
}

#[test]
fn detect_media_type_infers_a_docker_manifest_list_from_manifests_array() {
    let json = r#"{"schemaVersion": 2, "manifests": [{"mediaType": "application/vnd.docker.distribution.manifest.v2+json"}]}"#;
    assert_eq!(
        detect_manifest_media_type(json.as_bytes()),
        Some(DOCKER_MANIFEST_LIST_V2)
    );
}

#[test]
fn detect_media_type_infers_an_oci_index_when_any_sub_manifest_is_oci() {
    let json = r#"{"schemaVersion": 2, "manifests": [{"mediaType": "application/vnd.oci.image.manifest.v1+json"}]}"#;
    assert_eq!(
        detect_manifest_media_type(json.as_bytes()),
        Some(OCI_IMAGE_INDEX_V1)
    );
}

#[test]
fn detect_media_type_infers_a_docker_image_manifest_from_config_and_layers() {
    let json = r#"{"schemaVersion": 2, "config": {"mediaType": "application/vnd.docker.container.image.v1+json"}, "layers": []}"#;
    assert_eq!(
        detect_manifest_media_type(json.as_bytes()),
        Some(DOCKER_MANIFEST_V2)
    );
}

#[test]
fn detect_media_type_infers_an_oci_image_manifest_when_config_is_oci() {
    let json = r#"{"schemaVersion": 2, "config": {"mediaType": "application/vnd.oci.image.config.v1+json"}, "layers": []}"#;
    assert_eq!(
        detect_manifest_media_type(json.as_bytes()),
        Some(OCI_IMAGE_MANIFEST_V1)
    );
}

#[test]
fn detect_media_type_rejects_garbage_json() {
    assert_eq!(detect_manifest_media_type(b"not json"), None);
    assert_eq!(detect_manifest_media_type(br#"{"schemaVersion": 2}"#), None);
}

// -----------------------------------------------------------------
// parse_accept / media_match / negotiate_media_type
// -----------------------------------------------------------------

#[test]
fn parse_accept_reads_value_and_q_for_each_entry() {
    let ranges = parse_accept("application/json;q=0.5, */*;q=0.1");
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0].value, "application/json");
    assert!((ranges[0].q - 0.5).abs() < 1e-6);
    assert_eq!(ranges[1].value, "*/*");
    assert!((ranges[1].q - 0.1).abs() < 1e-6);
}

#[test]
fn parse_accept_defaults_q_to_one_when_absent() {
    let ranges = parse_accept("application/json");
    assert!((ranges[0].q - 1.0).abs() < 1e-6);
}

#[test]
fn parse_accept_skips_blank_entries() {
    let ranges = parse_accept("application/json, , text/plain");
    assert_eq!(ranges.len(), 2);
}

#[test]
fn media_match_accepts_a_wildcard_range() {
    assert!(media_match("*/*", DOCKER_MANIFEST_V2));
}

#[test]
fn media_match_accepts_an_exact_case_insensitive_match() {
    assert!(media_match(
        &DOCKER_MANIFEST_V2.to_ascii_uppercase(),
        DOCKER_MANIFEST_V2
    ));
}

#[test]
fn media_match_treats_docker_and_oci_manifest_types_as_equivalent() {
    assert!(media_match(OCI_IMAGE_MANIFEST_V1, DOCKER_MANIFEST_V2));
    assert!(media_match(DOCKER_MANIFEST_LIST_V2, OCI_IMAGE_INDEX_V1));
}

#[test]
fn media_match_accepts_a_type_wildcard_prefix() {
    assert!(media_match("application/*", DOCKER_MANIFEST_V2));
    assert!(!media_match("text/*", DOCKER_MANIFEST_V2));
}

#[test]
fn media_match_rejects_an_unrelated_type() {
    assert!(!media_match("text/html", DOCKER_MANIFEST_V2));
}

#[test]
fn negotiate_media_type_returns_the_first_available_when_accept_is_empty() {
    assert_eq!(
        negotiate_media_type("", &[DOCKER_MANIFEST_V2]),
        Some(DOCKER_MANIFEST_V2)
    );
}

#[test]
fn negotiate_media_type_prefers_the_highest_q_value() {
    let accept = "text/html;q=0.1, application/vnd.docker.distribution.manifest.v2+json;q=0.9";
    assert_eq!(
        negotiate_media_type(accept, &[DOCKER_MANIFEST_V2]),
        Some(DOCKER_MANIFEST_V2)
    );
}

#[test]
fn negotiate_media_type_is_none_when_nothing_matches() {
    assert_eq!(
        negotiate_media_type("text/html", &[DOCKER_MANIFEST_V2]),
        None
    );
}

// -----------------------------------------------------------------
// handle / resolve_manifest_response
// -----------------------------------------------------------------

fn write_manifest_and_tag(
    storage: &WithStorageRoot,
    repo: &str,
    tag: &str,
    manifest: &[u8],
) -> String {
    let hex = sha256_hex(manifest);
    let digest = format!("sha256:{hex}");

    let manifests_dir = storage.dir.path().join("manifests").join("sha256");
    std::fs::create_dir_all(&manifests_dir).unwrap();
    std::fs::write(manifests_dir.join(&hex), manifest).unwrap();

    let tags_dir = storage.dir.path().join(repo).join("tags");
    std::fs::create_dir_all(&tags_dir).unwrap();
    std::fs::write(tags_dir.join(tag), &digest).unwrap();

    digest
}

const MANIFEST_JSON: &str = r#"{"schemaVersion": 2, "mediaType": "application/vnd.docker.distribution.manifest.v2+json", "config": {}, "layers": []}"#;

#[actix_web::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri("/..%2fetc/manifests/latest")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn handle_reports_manifest_unknown_for_a_missing_tag() {
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri("/my-repo/manifests/latest")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn handle_serves_a_manifest_resolved_by_tag() {
    let storage = WithStorageRoot::new();
    let digest = write_manifest_and_tag(&storage, "my-repo", "latest", MANIFEST_JSON.as_bytes());

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri("/my-repo/manifests/latest")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert_eq!(
        resp.headers().get("Docker-Content-Digest").unwrap(),
        digest.as_str()
    );
    let body = actix_test::read_body(resp).await;
    assert_eq!(&body[..], MANIFEST_JSON.as_bytes());
}

#[actix_web::test]
async fn handle_serves_a_manifest_resolved_directly_by_digest() {
    let storage = WithStorageRoot::new();
    let digest = write_manifest_and_tag(&storage, "my-repo", "latest", MANIFEST_JSON.as_bytes());

    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri(&format!("/my-repo/manifests/{digest}"))
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn handle_rejects_a_tag_reference_containing_a_backslash() {
    // `validate_tag_reference` only rejects a backslash or more than one
    // path component - a tag like `not..valid` has neither (no `/` in
    // it, so `Path::new` sees one `Normal` component) and is syntactically
    // "valid" even though no such tag exists; that case is covered by
    // `handle_reports_manifest_unknown_for_a_missing_tag` instead.
    let _storage = WithStorageRoot::new();
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri("/my-repo/manifests/a%5Cb")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}
