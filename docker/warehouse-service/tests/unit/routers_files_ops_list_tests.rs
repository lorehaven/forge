use crate::support;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::JwtConfig;
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::files;
use warehouse_service::routers::files::ops::list::list_path;

#[tokio::test]
async fn storages_reports_not_found_when_file_storage_is_disabled() {
    // `FEATURE_FILES_ENABLED` is unset in this sandbox, and the flag is a
    // `LazyLock` fixed for the whole test binary, so this is the one branch
    // reachable deterministically.
    files::register_routes();
    let container = support::container_builder()
        .provide(JwtConfig::for_tests())
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container).await;

    let resp = app
        .call(support::req(Method::GET, "/api/v1/files", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn entries_reports_not_found_when_file_storage_is_disabled() {
    files::register_routes();
    let container = support::container_builder()
        .provide(JwtConfig::for_tests())
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container).await;

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/files/artifacts",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[test]
fn list_path_ascending_carries_only_the_prefix() {
    assert_eq!(
        list_path("backups", "photos/2026", false),
        "/api/v1/files/backups?prefix=photos%2F2026"
    );
}

#[test]
fn list_path_descending_also_carries_desc_true() {
    assert_eq!(
        list_path("backups", "photos/2026", true),
        "/api/v1/files/backups?prefix=photos%2F2026&desc=true"
    );
}

#[test]
fn list_path_encodes_a_prefix_with_reserved_characters() {
    assert_eq!(
        list_path("backups", "a b&c", false),
        "/api/v1/files/backups?prefix=a%20b%26c"
    );
}
