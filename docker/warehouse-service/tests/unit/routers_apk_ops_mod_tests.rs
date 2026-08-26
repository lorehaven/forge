use chrono::Utc;
use sqlx::types::Json;
use warehouse_service::domain::apk::ApkVersion;
use warehouse_service::routers::apk::ops::{VersionView, latest_of};

fn version(package_name: &str, version_code: i64, yanked: bool) -> ApkVersion {
    ApkVersion {
        id: ApkVersion::id_for(package_name, version_code),
        package_name: package_name.to_string(),
        version_code,
        version_name: format!("{version_code}.0"),
        min_sdk_version: Some(21),
        target_sdk_version: Some(34),
        label: Some("Test App".to_string()),
        permissions: Json(vec!["android.permission.INTERNET".to_string()]),
        size_bytes: 1024,
        sha256: "deadbeef".to_string(),
        uploaded_by: "dev".to_string(),
        yanked,
        created_at: Utc::now(),
    }
}

#[test]
fn latest_of_picks_the_highest_version_code() {
    let versions = vec![
        version("com.example.app", 1, false),
        version("com.example.app", 3, false),
        version("com.example.app", 2, false),
    ];

    let latest = latest_of(&versions).expect("a non-yanked version exists");
    assert_eq!(latest.version_code, 3);
}

#[test]
fn latest_of_skips_yanked_versions() {
    let versions = vec![
        version("com.example.app", 1, false),
        version("com.example.app", 2, true),
    ];

    let latest = latest_of(&versions).expect("version 1 is not yanked");
    assert_eq!(latest.version_code, 1);
}

#[test]
fn latest_of_is_none_when_every_version_is_yanked() {
    let versions = vec![version("com.example.app", 1, true)];
    assert!(latest_of(&versions).is_none());
}

#[test]
fn version_view_exposes_permissions_as_a_plain_vec_and_hides_the_storage_key() {
    let version = version("com.example.app", 5, false);
    let view = VersionView::from(&version);

    assert_eq!(view.package_name, "com.example.app");
    assert_eq!(view.version_code, 5);
    assert_eq!(view.permissions, vec!["android.permission.INTERNET"]);

    let json = serde_json::to_value(&view).expect("serializable");
    assert!(json.get("id").is_none(), "id should not be exposed: {json}");
}
