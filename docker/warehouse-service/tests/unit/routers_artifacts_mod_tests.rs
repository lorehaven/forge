use crate::support::WithArtifactStorageRoot;
use warehouse_service::domain::artifact::Platform;
use warehouse_service::routers::artifacts::{
    artifact_file_path, artifact_staging_path, default_filename, validate_filename,
    validate_program,
};

#[test]
fn accepts_ordinary_reverse_dns_program_ids() {
    assert!(validate_program("com.example.forge.testapp"));
    assert!(validate_program("a"));
    assert!(validate_program("_underscore.ok_2"));
}

#[test]
fn rejects_empty_leading_trailing_and_doubled_dots() {
    assert!(!validate_program(""));
    assert!(!validate_program(".leading"));
    assert!(!validate_program("trailing."));
    assert!(!validate_program("double..dot"));
}

#[test]
fn rejects_a_segment_starting_with_a_digit() {
    assert!(!validate_program("com.1example.app"));
}

#[test]
fn rejects_traversal_and_path_separators() {
    assert!(!validate_program("../../etc/passwd"));
    assert!(!validate_program("com/example"));
}

#[test]
fn rejects_names_over_255_characters() {
    let long = "a.".repeat(130);
    assert!(!validate_program(&long));
}

#[test]
fn filename_must_be_one_separator_free_component() {
    assert!(validate_filename("pedlar-7.tar.gz"));
    assert!(validate_filename("Mathom Setup 1.2.3.msi"));
    assert!(!validate_filename(""));
    assert!(!validate_filename(".."));
    assert!(!validate_filename("a/b"));
    assert!(!validate_filename("a\\b"));
}

#[test]
fn default_filename_uses_program_version_and_format() {
    assert_eq!(
        default_filename("com.example.app", 42, "apk"),
        "com.example.app-42.apk"
    );
    assert_eq!(
        default_filename("com.example.app", 7, "tar.gz"),
        "com.example.app-7.tar.gz"
    );
}

#[test]
fn artifact_file_path_lays_out_program_platform_version_then_filename() {
    let root = WithArtifactStorageRoot::new();
    let path = artifact_file_path(
        "com.example.app",
        Platform::Linux,
        42,
        "com.example.app-42.tar.gz",
    )
    .expect("valid name");

    assert_eq!(
        path,
        root.dir
            .path()
            .join("com.example.app")
            .join("linux")
            .join("42")
            .join("com.example.app-42.tar.gz")
    );
}

#[test]
fn artifact_file_path_rejects_an_invalid_program_or_filename() {
    assert_eq!(
        artifact_file_path("../escape", Platform::Android, 1, "x.apk"),
        None
    );
    assert_eq!(
        artifact_file_path("com.example.app", Platform::Android, 1, "../x.apk"),
        None
    );
}

#[test]
fn staging_path_calls_never_collide_even_for_the_same_target() {
    let a = artifact_staging_path(
        "com.example.app",
        Platform::Android,
        1,
        "com.example.app-1.apk",
    )
    .expect("valid name");
    let b = artifact_staging_path(
        "com.example.app",
        Platform::Android,
        1,
        "com.example.app-1.apk",
    )
    .expect("valid name");
    assert_ne!(a, b);
    assert!(a.to_string_lossy().contains(".part."));
}
