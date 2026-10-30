use quench_db::prelude::Model;
use warehouse_service::domain::artifact::{ArtifactVersion, Platform};

#[quench_test::test]
fn id_for_joins_program_platform_format_and_version_code() {
    assert_eq!(
        ArtifactVersion::id_for("com.example.app", Platform::Android, "apk", 42),
        "com.example.app/android/apk@42"
    );
    assert_eq!(
        ArtifactVersion::id_for("com.example.app", Platform::Linux, "deb", 7),
        "com.example.app/linux/deb@7"
    );
    assert_ne!(
        ArtifactVersion::id_for("com.example.app", Platform::Linux, "deb", 7),
        ArtifactVersion::id_for("com.example.app", Platform::Linux, "rpm", 7),
        "the same version in two formats is two rows"
    );
}

#[quench_test::test]
fn platform_round_trips_through_str() {
    for platform in Platform::ALL {
        assert_eq!(Platform::parse(platform.as_str()), Some(platform));
    }
    assert_eq!(Platform::parse("solaris"), None);
}

#[quench_test::test]
fn only_android_identity_is_verifiable() {
    assert!(Platform::Android.identity_is_verifiable());
    assert!(!Platform::Linux.identity_is_verifiable());
    assert!(!Platform::Windows.identity_is_verifiable());
}

#[quench_test::test]
fn columns_list_includes_every_field_including_the_primary_key() {
    let columns = ArtifactVersion::columns();
    for expected in [
        "id",
        "program",
        "platform",
        "arch",
        "format",
        "version_code",
        "filename",
        "metadata",
        "yanked",
    ] {
        assert!(columns.contains(&expected), "missing column {expected}");
    }
}

#[quench_test::test]
fn primary_key_name_defaults_to_id() {
    assert_eq!(ArtifactVersion::primary_key_name(), "id");
}
