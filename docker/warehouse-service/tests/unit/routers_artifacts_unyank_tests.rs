use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web;
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::artifacts::ops::unyank::handle;

#[actix_web::test]
async fn handle_reports_not_found_when_artifact_storage_is_disabled() {
    let app = actix_test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(Db::InMemory(InMemoryDb::new())))
            .service(handle),
    )
    .await;
    let req = actix_test::TestRequest::put()
        .uri("/com.example.app/android/1/unyank")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
