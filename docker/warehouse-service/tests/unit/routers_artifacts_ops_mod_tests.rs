use chrono::Utc;
use sqlx::types::Json;
use warehouse_service::domain::artifact::{ArtifactMetadata, ArtifactVersion, Platform};
use warehouse_service::routers::artifacts::ops::{ArtifactView, latest_of};

fn version(program: &str, platform: Platform, version_code: i64, yanked: bool) -> ArtifactVersion {
    ArtifactVersion {
        id: ArtifactVersion::id_for(program, platform, version_code),
        program: program.to_string(),
        platform: platform.as_str().to_string(),
        arch: (platform != Platform::Android).then(|| "x86_64".to_string()),
        format: if platform == Platform::Android {
            "apk"
        } else {
            "tar.gz"
        }
        .to_string(),
        version_code,
        version_name: format!("{version_code}.0"),
        filename: format!("{program}-{version_code}"),
        size_bytes: 1024,
        sha256: "deadbeef".to_string(),
        label: Some("Test App".to_string()),
        metadata: Json(if platform == Platform::Android {
            ArtifactMetadata {
                min_sdk_version: Some(21),
                target_sdk_version: Some(34),
                permissions: vec!["android.permission.INTERNET".to_string()],
            }
        } else {
            ArtifactMetadata::default()
        }),
        uploaded_by: "dev".to_string(),
        yanked,
        created_at: Utc::now(),
    }
}

#[test]
fn latest_of_picks_the_highest_version_code() {
    let versions = vec![
        version("com.example.app", Platform::Android, 1, false),
        version("com.example.app", Platform::Android, 3, false),
        version("com.example.app", Platform::Android, 2, false),
    ];
    assert_eq!(latest_of(&versions).unwrap().version_code, 3);
}

#[test]
fn latest_of_skips_yanked_versions() {
    let versions = vec![
        version("com.example.app", Platform::Android, 1, false),
        version("com.example.app", Platform::Android, 2, true),
    ];
    assert_eq!(latest_of(&versions).unwrap().version_code, 1);
}

#[test]
fn latest_of_is_none_when_every_version_is_yanked() {
    let versions = vec![version("com.example.app", Platform::Android, 1, true)];
    assert!(latest_of(&versions).is_none());
}

#[test]
fn artifact_view_flattens_metadata_and_hides_the_storage_key() {
    let view = ArtifactView::from(&version("com.example.app", Platform::Android, 5, false));

    assert_eq!(view.program, "com.example.app");
    assert_eq!(view.platform, "android");
    assert_eq!(view.version_code, 5);
    assert_eq!(view.min_sdk_version, Some(21));
    assert_eq!(view.permissions, vec!["android.permission.INTERNET"]);

    let json = serde_json::to_value(&view).expect("serializable");
    assert!(json.get("id").is_none(), "id should not be exposed: {json}");
    assert_eq!(
        json.get("platform").and_then(|v| v.as_str()),
        Some("android")
    );
}

#[test]
fn artifact_view_carries_arch_and_format_for_desktop_rows() {
    let view = ArtifactView::from(&version("com.example.app", Platform::Linux, 9, false));
    assert_eq!(view.platform, "linux");
    assert_eq!(view.arch.as_deref(), Some("x86_64"));
    assert_eq!(view.format, "tar.gz");
    assert!(view.permissions.is_empty());
}
