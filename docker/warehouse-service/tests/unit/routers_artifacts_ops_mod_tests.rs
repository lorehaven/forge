use quench_core::time::Time;
use quench_db::prelude::Crud;
use quench_db::{Db, InMemoryDb};
use quench_http::http::StatusCode;
use warehouse_service::domain::artifact::{ArtifactMetadata, ArtifactVersion, Platform};
use warehouse_service::routers::artifacts::ops::latest::resolve_latest;
use warehouse_service::routers::artifacts::ops::yank::set_yanked;
use warehouse_service::routers::artifacts::ops::{
    ArtifactView, FormatQuery, find_versions, latest_of, resolve_one,
};

fn version(program: &str, platform: Platform, version_code: i64, yanked: bool) -> ArtifactVersion {
    ArtifactVersion {
        id: ArtifactVersion::id_for(
            program,
            platform,
            if platform == Platform::Android {
                "apk"
            } else {
                "tar.gz"
            },
            version_code,
        ),
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
        metadata: if platform == Platform::Android {
            ArtifactMetadata {
                min_sdk_version: Some(21),
                target_sdk_version: Some(34),
                permissions: vec!["android.permission.INTERNET".to_string()],
            }
        } else {
            ArtifactMetadata::default()
        },
        uploaded_by: "dev".to_string(),
        yanked,
        created_at: Time::now(),
    }
}

#[quench_test::test]
fn latest_of_picks_the_highest_version_code() {
    let versions = vec![
        version("com.example.app", Platform::Android, 1, false),
        version("com.example.app", Platform::Android, 3, false),
        version("com.example.app", Platform::Android, 2, false),
    ];
    assert_eq!(latest_of(&versions).unwrap().version_code, 3);
}

#[quench_test::test]
fn latest_of_skips_yanked_versions() {
    let versions = vec![
        version("com.example.app", Platform::Android, 1, false),
        version("com.example.app", Platform::Android, 2, true),
    ];
    assert_eq!(latest_of(&versions).unwrap().version_code, 1);
}

#[quench_test::test]
fn latest_of_is_none_when_every_version_is_yanked() {
    let versions = vec![version("com.example.app", Platform::Android, 1, true)];
    assert!(latest_of(&versions).is_none());
}

#[quench_test::test]
fn artifact_view_flattens_metadata_and_hides_the_storage_key() {
    let view = ArtifactView::from(&version("com.example.app", Platform::Android, 5, false));

    assert_eq!(view.program, "com.example.app");
    assert_eq!(view.platform, "android");
    assert_eq!(view.version_code, 5);
    assert_eq!(view.min_sdk_version, Some(21));
    assert_eq!(view.permissions, vec!["android.permission.INTERNET"]);

    let json = quench_serde::to_value(&view);
    assert!(json.get("id").is_none(), "id should not be exposed: {json}");
    assert_eq!(
        json.get("platform").and_then(|v| v.as_str()),
        Some("android")
    );
}

#[quench_test::test]
fn artifact_view_carries_arch_and_format_for_desktop_rows() {
    let view = ArtifactView::from(&version("com.example.app", Platform::Linux, 9, false));
    assert_eq!(view.platform, "linux");
    assert_eq!(view.arch.as_deref(), Some("x86_64"));
    assert_eq!(view.format, "tar.gz");
    assert!(view.permissions.is_empty());
}

/// A Linux build of `code` in `format`.
fn linux(code: i64, format: &str) -> ArtifactVersion {
    let mut v = version("dev.example.app", Platform::Linux, code, false);
    v.format = format.to_string();
    v.id = ArtifactVersion::id_for("dev.example.app", Platform::Linux, format, code);
    v.filename = format!("app-{code}.{format}");
    v
}

/// The packages of two releases: a `.deb`, an `.rpm` and a source tarball of 1000, a `.deb` of 1001.
async fn seeded() -> Db {
    let db = Db::InMemory(InMemoryDb::new());
    let repo = db.repository::<ArtifactVersion>();
    for v in [
        linux(1000, "deb"),
        linux(1000, "rpm"),
        linux(1000, "tar.gz"),
        linux(1001, "deb"),
    ] {
        repo.create(&v).await.unwrap();
    }
    db
}

#[quench_test::test]
async fn one_version_can_hold_a_build_in_each_format() {
    let db = seeded().await;
    let all = find_versions(&db, "dev.example.app", Platform::Linux, 1000, None)
        .await
        .unwrap();
    assert_eq!(all.len(), 3);
    let rpm = find_versions(&db, "dev.example.app", Platform::Linux, 1000, Some("rpm"))
        .await
        .unwrap();
    assert_eq!(rpm.len(), 1);
    assert_eq!(rpm[0].format, "rpm");
}

#[quench_test::test]
async fn asking_for_a_version_without_a_format_is_ambiguous_only_when_it_is() {
    let db = seeded().await;
    // 1001 has one build: no format needed.
    let only = resolve_one(&db, "dev.example.app", Platform::Linux, 1001, None)
        .await
        .unwrap();
    assert_eq!(only.format, "deb");
    // 1000 has three: a 409 that names them.
    let err = resolve_one(&db, "dev.example.app", Platform::Linux, 1000, None)
        .await
        .unwrap_err();
    assert_eq!(err.status(), StatusCode::CONFLICT);
    // Naming the format picks one; a format it has not been built in is not found.
    let rpm = resolve_one(&db, "dev.example.app", Platform::Linux, 1000, Some("rpm"))
        .await
        .unwrap();
    assert_eq!(rpm.format, "rpm");
    let missing = resolve_one(&db, "dev.example.app", Platform::Linux, 1001, Some("rpm"))
        .await
        .unwrap_err();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn latest_is_per_format_when_one_is_asked_for() {
    let db = seeded().await;
    let any = resolve_latest(&db, "dev.example.app", "linux", None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(any.version_code, 1001);
    let rpm = resolve_latest(&db, "dev.example.app", "linux", Some("rpm"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rpm.version_code, 1000, "the rpm was not rebuilt for 1001");
    assert!(
        resolve_latest(&db, "dev.example.app", "linux", Some("pkg.tar.zst"))
            .await
            .unwrap()
            .is_none()
    );
}

#[quench_test::test]
async fn yanking_withdraws_every_format_unless_one_is_named() {
    let db = seeded().await;
    let repo = db.repository::<ArtifactVersion>();
    let state = |format: &'static str| {
        let repo = db.repository::<ArtifactVersion>();
        async move {
            repo.read(&ArtifactVersion::id_for(
                "dev.example.app",
                Platform::Linux,
                format,
                1000,
            ))
            .await
            .unwrap()
            .unwrap()
            .yanked
        }
    };

    let one = set_yanked(&db, "dev.example.app", "linux", 1000, Some("rpm"), true).await;
    assert_eq!(one.status(), StatusCode::OK);
    assert!(state("rpm").await && !state("deb").await && !state("tar.gz").await);

    let all = set_yanked(&db, "dev.example.app", "linux", 1000, None, true).await;
    assert_eq!(all.status(), StatusCode::OK);
    assert!(state("rpm").await && state("deb").await && state("tar.gz").await);

    let back = set_yanked(&db, "dev.example.app", "linux", 1000, None, false).await;
    assert_eq!(back.status(), StatusCode::OK);
    assert!(!state("rpm").await && !state("deb").await);

    let gone = set_yanked(&db, "dev.example.app", "linux", 4242, None, true).await;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
    let _ = repo;
}

#[quench_test::test]
fn the_format_query_is_trimmed_lowercased_and_may_be_blank() {
    let q = |f: Option<&str>| FormatQuery {
        format: f.map(str::to_string),
    };
    assert_eq!(q(Some(" RPM ")).wanted().as_deref(), Some("rpm"));
    assert_eq!(q(Some("  ")).wanted(), None);
    assert_eq!(q(None).wanted(), None);
}
