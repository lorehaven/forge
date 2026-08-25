use actix_web::App;
use actix_web::test as actix_test;
use actix_web::web;
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::files::ops::download::{download_name, handle, head, is_file};

fn in_memory_db() -> web::Data<Db> {
    web::Data::new(Db::InMemory(InMemoryDb::new()))
}

#[test]
fn download_name_uses_the_target_s_file_name() {
    assert_eq!(
        download_name(std::path::Path::new("/a/b/report.pdf")),
        "report.pdf"
    );
}

#[test]
fn download_name_strips_quotes_backslashes_and_control_bytes() {
    assert_eq!(
        download_name(std::path::Path::new("weird\"name\\with\x01control")),
        "weirdnamewithcontrol"
    );
}

#[test]
fn download_name_falls_back_to_download_when_nothing_usable_remains() {
    assert_eq!(download_name(std::path::Path::new("/")), "download");
    assert_eq!(download_name(std::path::Path::new("\"\\")), "download");
}

#[tokio::test]
async fn is_file_is_true_only_for_a_real_file_not_a_directory_or_missing_path() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    tokio::fs::write(&file, b"hi").await.unwrap();

    assert!(is_file(&file).await);
    assert!(!is_file(&dir.path().to_path_buf()).await);
    assert!(!is_file(&dir.path().join("missing")).await);
}

#[actix_web::test]
async fn handle_reports_not_found_when_file_storage_is_disabled() {
    // `FEATURE_FILES_ENABLED` is unset in this sandbox, and the flag is
    // a `LazyLock` fixed for the whole test binary (see `routers_mod_tests`'s
    // own tests) - so this deterministically hits the "not enabled"
    // branch rather than ever reaching the filesystem.
    let app = actix_test::init_service(App::new().app_data(in_memory_db()).service(handle)).await;
    let req = actix_test::TestRequest::get()
        .uri("/artifacts/file?path=a.txt")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn head_reports_not_found_when_file_storage_is_disabled() {
    let app = actix_test::init_service(App::new().app_data(in_memory_db()).service(head)).await;
    let req = actix_test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/artifacts/file?path=a.txt")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}
