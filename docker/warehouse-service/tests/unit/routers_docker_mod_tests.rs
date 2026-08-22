use crate::support;

use warehouse_service::routers::docker::{
    blob_exists, blob_path, digest_hex, manifest_path, repository_path, upload_path,
    validate_digest, validate_repository_name, validate_tag_reference,
};
use warehouse_service::routers::docker_storage_root;

const VALID_DIGEST: &str =
    "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn validate_digest_accepts_a_well_formed_sha256() {
    assert!(validate_digest(VALID_DIGEST));
}

#[test]
fn validate_digest_rejects_wrong_algorithm_length_or_characters() {
    assert!(!validate_digest(
        "md5:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    ));
    assert!(!validate_digest("sha256:tooshort"));
    assert!(!validate_digest(
        "sha256:g3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85"
    ));
    assert!(!validate_digest(""));
}

#[test]
fn digest_hex_strips_the_algorithm_prefix_only_when_valid() {
    assert_eq!(
        digest_hex(VALID_DIGEST),
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
    assert_eq!(digest_hex("not-a-digest"), None);
}

#[test]
fn validate_repository_name_rejects_traversal_and_backslashes() {
    assert!(validate_repository_name("my/repo"));
    assert!(!validate_repository_name(""));
    assert!(!validate_repository_name("../etc"));
    assert!(!validate_repository_name("a\\b"));
    assert!(!validate_repository_name("./repo"));
}

#[test]
fn validate_tag_reference_accepts_exactly_one_normal_component() {
    assert!(validate_tag_reference("latest"));
    assert!(validate_tag_reference("v1.2.3"));
    assert!(!validate_tag_reference(""));
    assert!(!validate_tag_reference("a/b"));
    assert!(!validate_tag_reference("a\\b"));
    assert!(!validate_tag_reference(".."));
}

#[test]
fn repository_path_rejects_invalid_names_and_nests_valid_ones_under_the_root() {
    assert!(repository_path("../etc").is_none());
    let path = repository_path("my/repo").expect("valid name");
    assert_eq!(
        path.strip_prefix(docker_storage_root()).unwrap(),
        std::path::Path::new("my/repo")
    );
}

#[test]
fn blob_path_and_manifest_path_use_separate_sha256_subtrees() {
    let blob = blob_path(VALID_DIGEST).expect("valid digest");
    let manifest = manifest_path(VALID_DIGEST).expect("valid digest");

    assert!(
        blob.strip_prefix(docker_storage_root())
            .unwrap()
            .starts_with("blobs/sha256")
    );
    assert!(
        manifest
            .strip_prefix(docker_storage_root())
            .unwrap()
            .starts_with("manifests/sha256")
    );
    assert_ne!(blob, manifest);
}

#[test]
fn blob_path_rejects_an_invalid_digest() {
    assert!(blob_path("not-a-digest").is_none());
}

#[test]
fn upload_path_nests_under_the_repository_s_uploads_directory() {
    let path = upload_path("my/repo", "upload-uuid").expect("valid repo");
    assert_eq!(
        path.strip_prefix(docker_storage_root()).unwrap(),
        std::path::Path::new("my/repo/_uploads/upload-uuid")
    );
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn blob_exists_is_false_for_a_digest_with_nothing_on_disk() {
    let _guard = support::storage_env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(!blob_exists(VALID_DIGEST).await);
}

#[tokio::test]
async fn blob_exists_is_false_for_an_invalid_digest() {
    assert!(!blob_exists("not-a-digest").await);
}
