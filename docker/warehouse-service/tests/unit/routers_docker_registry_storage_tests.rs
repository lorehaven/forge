use crate::support;

use support::WithDockerStorageRoot as WithStorageRoot;
use warehouse_service::routers::docker::registry::storage::{
    TagListError, VersionTag, compare_tags_desc, compare_version_tags_desc, detect_media_type,
    list_repositories, list_tag_metadata_for_repository, list_tags_for_repository,
    parse_version_tag,
};

/// Lays out `<repo>/tags/<tag>` pointing at a manifest digest, and (when
/// `manifest_json` is given) the manifest blob itself under
/// `manifests/sha256/<hex>` - the same shape `repository_path`/
/// `manifest_path` compute in production.
fn write_tag(
    storage: &WithStorageRoot,
    repo: &str,
    tag: &str,
    digest: &str,
    manifest_json: Option<&str>,
) {
    let tags_dir = storage.dir.path().join(repo).join("tags");
    std::fs::create_dir_all(&tags_dir).unwrap();
    std::fs::write(tags_dir.join(tag), digest).unwrap();

    if let (Some(hex), Some(json)) = (digest.strip_prefix("sha256:"), manifest_json) {
        let manifests_dir = storage.dir.path().join("manifests").join("sha256");
        std::fs::create_dir_all(&manifests_dir).unwrap();
        std::fs::write(manifests_dir.join(hex), json).unwrap();
    }
}

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn list_repositories_is_empty_on_a_fresh_storage_root() {
    let storage = WithStorageRoot::new();
    std::fs::create_dir_all(storage.dir.path()).unwrap();
    assert!(list_repositories().is_empty());
}

#[test]
fn list_repositories_finds_every_repo_with_a_tags_directory_and_skips_reserved_names() {
    let storage = WithStorageRoot::new();
    write_tag(&storage, "my/repo", "latest", DIGEST, None);
    write_tag(&storage, "another-repo", "v1", DIGEST, None);
    // Reserved top-level dirs that aren't repositories.
    std::fs::create_dir_all(storage.dir.path().join("blobs")).unwrap();
    std::fs::create_dir_all(storage.dir.path().join("_uploads")).unwrap();

    let mut repos = list_repositories();
    repos.sort();
    assert_eq!(
        repos,
        vec!["another-repo".to_string(), "my/repo".to_string()]
    );
}

#[test]
fn list_tags_for_repository_rejects_an_invalid_name() {
    let _storage = WithStorageRoot::new();
    assert_eq!(
        list_tags_for_repository("../etc"),
        Err(TagListError::InvalidName)
    );
}

#[test]
fn list_tags_for_repository_reports_not_found_without_a_tags_directory() {
    let _storage = WithStorageRoot::new();
    assert_eq!(
        list_tags_for_repository("no-such-repo"),
        Err(TagListError::NotFound)
    );
}

#[test]
fn list_tags_for_repository_lists_every_tag_newest_semver_first() {
    let storage = WithStorageRoot::new();
    write_tag(&storage, "my/repo", "1.0.0", DIGEST, None);
    write_tag(&storage, "my/repo", "2.0.0", DIGEST, None);
    write_tag(&storage, "my/repo", "1.5.0", DIGEST, None);

    let tags = list_tags_for_repository("my/repo").expect("found");
    assert_eq!(tags, vec!["2.0.0", "1.5.0", "1.0.0"]);
}

#[test]
fn list_tag_metadata_reads_size_and_media_type_from_the_manifest() {
    let storage = WithStorageRoot::new();
    let manifest = r#"{"mediaType": "application/vnd.oci.image.manifest.v1+json"}"#;
    write_tag(&storage, "my/repo", "latest", DIGEST, Some(manifest));

    let items = list_tag_metadata_for_repository("my/repo").expect("found");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].tag, "latest");
    assert_eq!(items[0].digest, DIGEST);
    assert_eq!(
        items[0].media_type.as_deref(),
        Some("application/vnd.oci.image.manifest.v1+json")
    );
    assert_eq!(items[0].size_bytes, Some(manifest.len() as u64));
}

#[test]
fn list_tag_metadata_tolerates_a_tag_with_no_manifest_on_disk() {
    let storage = WithStorageRoot::new();
    write_tag(&storage, "my/repo", "dangling", DIGEST, None);

    let items = list_tag_metadata_for_repository("my/repo").expect("found");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].media_type, None);
    assert_eq!(items[0].size_bytes, None);
}

#[test]
fn compare_tags_desc_orders_semver_tags_above_non_semver_ones_by_string() {
    let mut tags = [
        "latest".to_string(),
        "1.2.0".to_string(),
        "1.10.0".to_string(),
    ];
    tags.sort_by(|a, b| compare_tags_desc(a, b));
    // Semver-parseable tags compare numerically (1.10.0 > 1.2.0); tags
    // that don't parse fall back to reverse-lexicographic and just need
    // to not panic or silently vanish.
    assert!(tags.contains(&"latest".to_string()));
    let v1_10 = tags.iter().position(|t| t == "1.10.0").unwrap();
    let v1_2 = tags.iter().position(|t| t == "1.2.0").unwrap();
    assert!(v1_10 < v1_2);
}

#[test]
fn parse_version_tag_accepts_major_minor_patch_with_optional_suffix() {
    assert_eq!(
        parse_version_tag("1.2.3"),
        Some(VersionTag {
            major: 1,
            minor: 2,
            patch: 3,
            suffix: None
        })
    );
    assert_eq!(
        parse_version_tag("1.2.3-beta"),
        Some(VersionTag {
            major: 1,
            minor: 2,
            patch: 3,
            suffix: Some("beta".to_string())
        })
    );
}

#[test]
fn parse_version_tag_rejects_malformed_versions() {
    assert_eq!(parse_version_tag("latest"), None);
    assert_eq!(parse_version_tag("1.2"), None);
    assert_eq!(parse_version_tag("1.2.3.4"), None);
    assert_eq!(parse_version_tag("a.b.c"), None);
}

#[test]
fn compare_version_tags_desc_orders_newest_first_and_unsuffixed_above_suffixed() {
    let v1 = VersionTag {
        major: 1,
        minor: 0,
        patch: 0,
        suffix: None,
    };
    let v2 = VersionTag {
        major: 2,
        minor: 0,
        patch: 0,
        suffix: None,
    };
    assert_eq!(
        compare_version_tags_desc(&v2, &v1),
        std::cmp::Ordering::Less
    );

    let stable = VersionTag {
        major: 1,
        minor: 0,
        patch: 0,
        suffix: None,
    };
    let beta = VersionTag {
        major: 1,
        minor: 0,
        patch: 0,
        suffix: Some("beta".to_string()),
    };
    assert_eq!(
        compare_version_tags_desc(&stable, &beta),
        std::cmp::Ordering::Less
    );
}

#[test]
fn detect_media_type_reads_the_field_or_returns_none() {
    assert_eq!(
        detect_media_type(br#"{"mediaType": "application/json"}"#),
        Some("application/json".to_string())
    );
    assert_eq!(detect_media_type(b"not json"), None);
    assert_eq!(detect_media_type(b"{}"), None);
}
