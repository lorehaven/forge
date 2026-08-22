use crate::support;

use std::collections::{HashSet, VecDeque};
use std::path::Path;
use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::admin::docker::gc::{
    garbage_collect, is_digest_file_path, is_sha256_hex, mark_manifest_references,
};

const DIGEST_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DIGEST_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

// -----------------------------------------------------------------
// is_sha256_hex / is_digest_file_path
// -----------------------------------------------------------------

#[test]
fn is_sha256_hex_accepts_exactly_64_hex_chars() {
    assert!(is_sha256_hex(DIGEST_A));
    assert!(!is_sha256_hex("tooshort"));
    assert!(!is_sha256_hex(&format!("{DIGEST_A}f")));
    assert!(!is_sha256_hex(
        "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg"
    ));
}

#[test]
fn is_digest_file_path_requires_a_sha256_subdirectory_under_the_given_kind() {
    assert!(is_digest_file_path(
        Path::new("/root/blobs/sha256/abc"),
        "blobs"
    ));
    assert!(!is_digest_file_path(
        Path::new("/root/blobs/sha1/abc"),
        "blobs"
    ));
    assert!(!is_digest_file_path(
        Path::new("/root/manifests/sha256/abc"),
        "blobs"
    ));
    assert!(!is_digest_file_path(Path::new("/abc"), "blobs"));
}

// -----------------------------------------------------------------
// mark_manifest_references
// -----------------------------------------------------------------

#[test]
fn mark_manifest_references_collects_config_and_layer_digests() {
    let manifest = format!(
        r#"{{"config": {{"digest": "sha256:{DIGEST_A}"}}, "layers": [{{"digest": "sha256:{DIGEST_B}"}}]}}"#
    );
    let mut referenced = HashSet::new();
    let mut to_visit = VecDeque::new();
    mark_manifest_references(manifest.as_bytes(), &mut referenced, &mut to_visit);

    assert!(referenced.contains(&format!("sha256:{DIGEST_A}")));
    assert!(referenced.contains(&format!("sha256:{DIGEST_B}")));
    assert!(to_visit.is_empty());
}

#[test]
fn mark_manifest_references_queues_a_subject_manifest_for_visiting() {
    let manifest = format!(r#"{{"subject": {{"digest": "sha256:{DIGEST_A}"}}}}"#);
    let mut referenced = HashSet::new();
    let mut to_visit = VecDeque::new();
    mark_manifest_references(manifest.as_bytes(), &mut referenced, &mut to_visit);

    assert_eq!(to_visit.len(), 1);
    assert_eq!(to_visit[0], format!("sha256:{DIGEST_A}"));
    assert!(referenced.is_empty());
}

#[test]
fn mark_manifest_references_treats_index_entries_as_manifests_to_visit() {
    let manifest = format!(
        r#"{{"manifests": [{{"digest": "sha256:{DIGEST_A}", "mediaType": "application/vnd.oci.image.manifest.v1+json"}}]}}"#
    );
    let mut referenced = HashSet::new();
    let mut to_visit = VecDeque::new();
    mark_manifest_references(manifest.as_bytes(), &mut referenced, &mut to_visit);

    assert_eq!(to_visit.len(), 1);
    assert!(referenced.is_empty());
}

#[test]
fn mark_manifest_references_treats_a_manifests_entry_with_an_unrelated_media_type_as_a_blob() {
    let manifest = format!(
        r#"{{"manifests": [{{"digest": "sha256:{DIGEST_A}", "mediaType": "application/octet-stream"}}]}}"#
    );
    let mut referenced = HashSet::new();
    let mut to_visit = VecDeque::new();
    mark_manifest_references(manifest.as_bytes(), &mut referenced, &mut to_visit);

    assert!(to_visit.is_empty());
    assert!(referenced.contains(&format!("sha256:{DIGEST_A}")));
}

#[test]
fn mark_manifest_references_ignores_invalid_json() {
    let mut referenced = HashSet::new();
    let mut to_visit = VecDeque::new();
    mark_manifest_references(b"not json", &mut referenced, &mut to_visit);
    assert!(referenced.is_empty());
    assert!(to_visit.is_empty());
}

// -----------------------------------------------------------------
// garbage_collect
// -----------------------------------------------------------------

fn write_manifest(storage: &WithStorageRoot, digest_hex: &str, content: &str) {
    let dir = storage.dir.path().join("manifests").join("sha256");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(digest_hex), content).unwrap();
}

fn write_blob(storage: &WithStorageRoot, digest_hex: &str) {
    let dir = storage.dir.path().join("blobs").join("sha256");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(digest_hex), b"blob content").unwrap();
}

#[tokio::test]
async fn garbage_collect_keeps_referenced_blobs_and_deletes_unreferenced_ones() {
    let storage = WithStorageRoot::new();

    const MANIFEST_DIGEST: &str =
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let manifest = format!(r#"{{"config": {{"digest": "sha256:{DIGEST_A}"}}, "layers": []}}"#);
    write_manifest(&storage, MANIFEST_DIGEST, &manifest);
    write_blob(&storage, DIGEST_A);
    write_blob(&storage, DIGEST_B);

    let report = garbage_collect().await.expect("gc succeeds");
    assert_eq!(report.kept, 1);
    assert_eq!(report.deleted, 1);

    assert!(
        storage
            .dir
            .path()
            .join("blobs/sha256")
            .join(DIGEST_A)
            .exists()
    );
    assert!(
        !storage
            .dir
            .path()
            .join("blobs/sha256")
            .join(DIGEST_B)
            .exists()
    );
}

#[tokio::test]
async fn garbage_collect_on_an_empty_storage_root_deletes_and_keeps_nothing() {
    let storage = WithStorageRoot::new();
    std::fs::create_dir_all(storage.dir.path()).unwrap();

    let report = garbage_collect().await.expect("gc succeeds");
    assert_eq!(report.deleted, 0);
    assert_eq!(report.kept, 0);
}
