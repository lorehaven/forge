use quench_db::prelude::Model;
use warehouse_service::domain::apk::ApkVersion;

#[test]
fn id_for_joins_package_and_version_code_with_at() {
    assert_eq!(
        ApkVersion::id_for("com.example.app", 42),
        "com.example.app@42"
    );
}

#[test]
fn columns_list_includes_every_field_including_the_primary_key() {
    let columns = ApkVersion::columns();
    assert!(columns.contains(&"id"));
    assert!(columns.contains(&"package_name"));
    assert!(columns.contains(&"version_code"));
    assert!(columns.contains(&"permissions"));
    assert!(columns.contains(&"yanked"));
}

#[test]
fn primary_key_name_defaults_to_id() {
    assert_eq!(ApkVersion::primary_key_name(), "id");
}
