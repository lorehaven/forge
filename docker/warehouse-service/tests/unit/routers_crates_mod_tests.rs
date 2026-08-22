use warehouse_service::routers::crates::{
    crate_file_path, index_file_path, index_prefix, validate_crate_name, validate_version,
};
use warehouse_service::routers::crates_storage_root;

#[test]
fn index_prefix_follows_the_crates_io_convention() {
    assert_eq!(index_prefix("a"), "1");
    assert_eq!(index_prefix("ab"), "2");
    assert_eq!(index_prefix("abc"), "3/a");
    assert_eq!(index_prefix("abcd"), "ab/cd");
    assert_eq!(index_prefix("abcdefgh"), "ab/cd");
}

#[test]
fn index_prefix_lowercases_the_name_first() {
    assert_eq!(index_prefix("ABCD"), "ab/cd");
}

#[test]
fn index_prefix_of_empty_is_empty() {
    assert_eq!(index_prefix(""), "");
}

#[test]
fn validate_crate_name_accepts_ascii_alphanumeric_dash_and_underscore() {
    assert!(validate_crate_name("my-crate_1"));
    assert!(validate_crate_name("a"));
}

#[test]
fn validate_crate_name_rejects_empty_too_long_or_bad_characters() {
    assert!(!validate_crate_name(""));
    assert!(!validate_crate_name(&"a".repeat(65)));
    assert!(validate_crate_name(&"a".repeat(64)));
    assert!(!validate_crate_name("../etc/passwd"));
    assert!(!validate_crate_name("has space"));
    assert!(!validate_crate_name("emoji🦀"));
}

#[test]
fn validate_version_accepts_semver_ish_strings() {
    assert!(validate_version("1.2.3"));
    assert!(validate_version("1.2.3-alpha.1+build.5"));
}

#[test]
fn validate_version_rejects_empty_too_long_or_path_traversal() {
    assert!(!validate_version(""));
    assert!(!validate_version(&"1".repeat(65)));
    assert!(!validate_version("../../etc/passwd"));
    assert!(!validate_version("1.2.3/../4"));
}

#[test]
fn crate_file_path_rejects_invalid_names_or_versions() {
    assert!(crate_file_path("../etc", "1.0.0").is_none());
    assert!(crate_file_path("ok-name", "../1.0.0").is_none());
}

#[test]
fn crate_file_path_lays_out_name_version_and_filename() {
    let path = crate_file_path("my-crate", "1.2.3").expect("valid inputs");
    assert_eq!(
        path.strip_prefix(crates_storage_root()).unwrap(),
        std::path::Path::new("my-crate/1.2.3/my-crate-1.2.3.crate")
    );
}

#[test]
fn index_file_path_rejects_invalid_names() {
    assert!(index_file_path("../etc").is_none());
    assert!(index_file_path("").is_none());
}

#[test]
fn index_file_path_nests_under_the_computed_prefix() {
    let path = index_file_path("my-crate").expect("valid name");
    assert_eq!(
        path.strip_prefix(crates_storage_root()).unwrap(),
        std::path::Path::new("index/my/-c/my-crate")
    );
}
