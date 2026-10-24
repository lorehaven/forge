use crate::support;
use quench_db::{Db, InMemoryDb};
use quench_http::http::{Method, StatusCode};
use warehouse_service::routers::artifacts;

#[quench_test::test]
async fn handle_reports_not_found_when_artifact_storage_is_disabled() {
    artifacts::register_routes();
    let container = support::container_builder()
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/artifacts/com.example.app/android/1/download",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

mod macos {
    use crate::support::WithArtifactStorageRoot;
    use quench_core::time::Time;
    use quench_http::http::StatusCode;
    use warehouse_service::domain::artifact::{ArtifactMetadata, ArtifactVersion, Platform};
    use warehouse_service::routers::artifacts::artifact_file_path;
    use warehouse_service::routers::artifacts::ops::download::{content_type_for, serve};

    fn dmg(program: &str, version_code: i64, filename: &str) -> ArtifactVersion {
        ArtifactVersion {
            id: ArtifactVersion::id_for(program, Platform::Macos, version_code),
            program: program.to_string(),
            platform: "macos".to_string(),
            arch: Some("aarch64".to_string()),
            format: "dmg".to_string(),
            version_code,
            version_name: "1.2.3".to_string(),
            filename: filename.to_string(),
            size_bytes: 9,
            sha256: "deadbeef".to_string(),
            label: None,
            metadata: ArtifactMetadata::default(),
            uploaded_by: "dev".to_string(),
            yanked: false,
            created_at: Time::now(),
        }
    }

    #[quench_test::test]
    fn disk_images_are_served_as_such_and_other_formats_as_opaque_bytes() {
        assert_eq!(content_type_for("dmg"), "application/x-apple-diskimage");
        assert_eq!(
            content_type_for("apk"),
            "application/vnd.android.package-archive"
        );
        for opaque in ["pkg", "zip", "tar.gz", "msi", "exe"] {
            assert_eq!(content_type_for(opaque), "application/octet-stream");
        }
    }

    #[quench_test::test]
    async fn a_published_macos_build_downloads_with_its_filename_and_type() {
        let _root = WithArtifactStorageRoot::new();
        let version = dmg("com.example.app", 7, "App-1.2.3.dmg");
        let path = artifact_file_path("com.example.app", Platform::Macos, 7, "App-1.2.3.dmg")
            .expect("valid name");
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, b"dmg-bytes").await.unwrap();

        let resp = serve(&version).await;

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/x-apple-diskimage"
        );
        assert_eq!(
            resp.headers().get("content-disposition").unwrap(),
            "attachment; filename=\"App-1.2.3.dmg\""
        );
    }

    #[quench_test::test]
    async fn a_macos_row_whose_file_is_gone_is_not_found() {
        let _root = WithArtifactStorageRoot::new();
        let resp = serve(&dmg("com.example.app", 8, "missing.dmg")).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
