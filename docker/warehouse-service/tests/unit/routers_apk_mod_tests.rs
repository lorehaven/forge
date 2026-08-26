use crate::support::WithApkStorageRoot;
use warehouse_service::routers::apk::{apk_file_path, apk_staging_path, validate_package_name};

#[test]
fn accepts_ordinary_java_style_package_names() {
    assert!(validate_package_name("com.example.forge.testapp"));
    assert!(validate_package_name("a"));
    assert!(validate_package_name("_underscore.ok_2"));
}

#[test]
fn rejects_empty_leading_trailing_and_doubled_dots() {
    assert!(!validate_package_name(""));
    assert!(!validate_package_name(".leading"));
    assert!(!validate_package_name("trailing."));
    assert!(!validate_package_name("double..dot"));
}

#[test]
fn rejects_a_segment_starting_with_a_digit() {
    assert!(!validate_package_name("com.1example.app"));
}

#[test]
fn rejects_traversal_and_path_separators() {
    assert!(!validate_package_name("../../etc/passwd"));
    assert!(!validate_package_name("com/example"));
}

#[test]
fn rejects_names_over_255_characters() {
    let long = "a.".repeat(130);
    assert!(!validate_package_name(&long));
}

#[test]
fn apk_file_path_lays_out_package_then_version_then_filename() {
    let root = WithApkStorageRoot::new();
    let path = apk_file_path("com.example.app", 42).expect("valid name");

    assert_eq!(
        path,
        root.dir
            .path()
            .join("com.example.app")
            .join("42")
            .join("com.example.app-42.apk")
    );
}

#[test]
fn apk_file_path_rejects_an_invalid_package_name() {
    assert_eq!(apk_file_path("../escape", 1), None);
}

#[test]
fn apk_staging_path_calls_never_collide_even_for_the_same_target() {
    let a = apk_staging_path("com.example.app", 1).expect("valid name");
    let b = apk_staging_path("com.example.app", 1).expect("valid name");
    assert_ne!(a, b);
    assert!(a.to_string_lossy().ends_with(".part"));
}
