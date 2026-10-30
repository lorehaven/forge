use crate::support;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::{Db, InMemoryDb};
use quench_http::http::{Method, StatusCode};
use warehouse_service::routers::files;
use warehouse_service::routers::files::ops::download::{download_name, is_file};

#[quench_test::test]
fn download_name_uses_the_target_s_file_name() {
    assert_eq!(
        download_name(std::path::Path::new("/a/b/report.pdf")),
        "report.pdf"
    );
}

#[quench_test::test]
fn download_name_strips_quotes_backslashes_and_control_bytes() {
    assert_eq!(
        download_name(std::path::Path::new("weird\"name\\with\x01control")),
        "weirdnamewithcontrol"
    );
}

#[quench_test::test]
fn download_name_falls_back_to_download_when_nothing_usable_remains() {
    assert_eq!(download_name(std::path::Path::new("/")), "download");
    assert_eq!(download_name(std::path::Path::new("\"\\")), "download");
}

#[quench_test::test]
async fn is_file_is_true_only_for_a_real_file_not_a_directory_or_missing_path() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let file = dir.path().join("f.txt");
    tokio::fs::write(&file, b"hi").await.unwrap();

    assert!(is_file(&file).await);
    assert!(!is_file(&dir.path().to_path_buf()).await);
    assert!(!is_file(&dir.path().join("missing")).await);
}

#[quench_test::test]
async fn handle_reports_not_found_when_file_storage_is_disabled() {
    // `FEATURE_FILES_ENABLED` is unset in this sandbox, and the flag is
    // a `LazyLock` fixed for the whole test binary - so this deterministically
    // hits the "not enabled" branch rather than ever reaching the filesystem.
    files::register_routes();
    let container = support::container_builder()
        .provide(JwtConfig::for_tests())
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/files/artifacts/file?path=a.txt",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn head_reports_not_found_when_file_storage_is_disabled() {
    files::register_routes();
    let container = support::container_builder()
        .provide(JwtConfig::for_tests())
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);

    let resp = app
        .call(support::req(
            Method::HEAD,
            "/api/v1/files/artifacts/file?path=a.txt",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
