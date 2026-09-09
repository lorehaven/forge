use quench_db::prelude::Model;
use warehouse_service::domain::artifact::{ArtifactVersion, Platform};

#[test]
fn id_for_joins_program_platform_and_version_code() {
    assert_eq!(
        ArtifactVersion::id_for("com.example.app", Platform::Android, 42),
        "com.example.app/android@42"
    );
    assert_eq!(
        ArtifactVersion::id_for("com.example.app", Platform::Linux, 7),
        "com.example.app/linux@7"
    );
}

#[test]
fn platform_round_trips_through_str() {
    for platform in Platform::ALL {
        assert_eq!(Platform::parse(platform.as_str()), Some(platform));
    }
    assert_eq!(Platform::parse("solaris"), None);
}

#[test]
fn only_android_identity_is_verifiable() {
    assert!(Platform::Android.identity_is_verifiable());
    assert!(!Platform::Linux.identity_is_verifiable());
    assert!(!Platform::Windows.identity_is_verifiable());
}

#[test]
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

#[test]
fn primary_key_name_defaults_to_id() {
    assert_eq!(ArtifactVersion::primary_key_name(), "id");
}
