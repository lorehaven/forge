use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web;
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::files::ops::list::{entries, list_path, storages};

fn in_memory_db() -> web::Data<Db> {
    web::Data::new(Db::InMemory(InMemoryDb::new()))
}

#[actix_web::test]
async fn storages_reports_not_found_when_file_storage_is_disabled() {
    // `FEATURE_FILES_ENABLED` is unset in this sandbox, and the flag is a
    // `LazyLock` fixed for the whole test binary (see `routers_mod_tests`'s
    // own tests), so this is the one branch reachable deterministically.
    let app = actix_test::init_service(
        actix_web::App::new()
            .app_data(in_memory_db())
            .service(storages),
    )
    .await;
    let req = actix_test::TestRequest::get().uri("/").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn entries_reports_not_found_when_file_storage_is_disabled() {
    let app = actix_test::init_service(
        actix_web::App::new()
            .app_data(in_memory_db())
            .service(entries),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/artifacts")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
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
