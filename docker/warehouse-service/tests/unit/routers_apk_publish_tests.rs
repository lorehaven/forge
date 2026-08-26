use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web;
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::apk::ops::publish::handle;

#[actix_web::test]
async fn handle_reports_not_found_when_apk_storage_is_disabled() {
    // `FEATURE_APK_ENABLED` is unset in this sandbox, and the flag is a
    // `LazyLock` fixed for the whole test binary (see
    // `routers_files_ops_download_tests`'s own comment for the same
    // reasoning) - so this deterministically hits the "not enabled" branch
    // rather than ever touching the filesystem or the database.
    let app = actix_test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(Db::InMemory(InMemoryDb::new())))
            .service(handle),
    )
    .await;
    let req = actix_test::TestRequest::put()
        .uri("/com.example.app/1")
        .set_payload(Vec::<u8>::new())
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
