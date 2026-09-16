use crate::support;

use http::{Method, StatusCode};
use warehouse_service::routers::docker::manifest::get_image::{
    DOCKER_MANIFEST_LIST_V2, DOCKER_MANIFEST_V2, OCI_IMAGE_INDEX_V1, OCI_IMAGE_MANIFEST_V1,
    detect_manifest_media_type, media_match, negotiate_media_type, parse_accept,
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
    storage: &support::WithDockerStorageRoot,
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

async fn app() -> (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
) {
    warehouse_service::routers::docker::manifest::get_image::register_routes();
    let container = support::container_builder().build().await.unwrap();
    support::app(container).await
}

#[tokio::test]
async fn handle_rejects_an_invalid_repository_name() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/v2/..%2fetc/manifests/latest",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handle_reports_manifest_unknown_for_a_missing_tag() {
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/v2/my-repo/manifests/latest",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn handle_serves_a_manifest_resolved_by_tag() {
    let storage = support::WithDockerStorageRoot::new();
    let digest = write_manifest_and_tag(&storage, "my-repo", "latest", MANIFEST_JSON.as_bytes());

    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/v2/my-repo/manifests/latest",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (headers, body) = support::parts(resp).await;
    assert_eq!(
        headers.get("docker-content-digest").unwrap(),
        digest.as_str()
    );
    assert_eq!(body, MANIFEST_JSON);
}

#[tokio::test]
async fn handle_serves_a_manifest_resolved_directly_by_digest() {
    let storage = support::WithDockerStorageRoot::new();
    let digest = write_manifest_and_tag(&storage, "my-repo", "latest", MANIFEST_JSON.as_bytes());

    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            &format!("/v2/my-repo/manifests/{digest}"),
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn handle_rejects_a_tag_reference_containing_a_backslash() {
    // `validate_tag_reference` only rejects a backslash or more than one
    // path component - a tag like `not..valid` has neither (no `/` in
    // it, so `Path::new` sees one `Normal` component) and is syntactically
    // "valid" even though no such tag exists; that case is covered by
    // `handle_reports_manifest_unknown_for_a_missing_tag` instead.
    let _storage = support::WithDockerStorageRoot::new();
    let (app, container) = app().await;
    let resp = app
        .call(support::req(
            Method::GET,
            "/v2/my-repo/manifests/a%5Cb",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
