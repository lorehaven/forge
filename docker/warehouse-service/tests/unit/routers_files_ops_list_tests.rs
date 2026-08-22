use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use warehouse_service::routers::files::ops::list::{entries, storages};

#[actix_web::test]
async fn storages_reports_not_found_when_file_storage_is_disabled() {
    // `FEATURE_FILES_ENABLED` is unset in this sandbox, and the flag is a
    // `LazyLock` fixed for the whole test binary (see `routers_mod_tests`'s
    // own tests), so this is the one branch reachable deterministically.
    let app = actix_test::init_service(actix_web::App::new().service(storages)).await;
    let req = actix_test::TestRequest::get().uri("/").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn entries_reports_not_found_when_file_storage_is_disabled() {
    let app = actix_test::init_service(actix_web::App::new().service(entries)).await;
    let req = actix_test::TestRequest::get()
        .uri("/artifacts")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
