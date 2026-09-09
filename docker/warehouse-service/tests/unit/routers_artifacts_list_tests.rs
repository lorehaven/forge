use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use actix_web::web;
use quench_db::{Db, InMemoryDb};
use warehouse_service::routers::artifacts::ops::list::{
    catalog, platform_versions, program_versions,
};

#[actix_web::test]
async fn platform_versions_reports_not_found_when_disabled() {
    let app = actix_test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(Db::InMemory(InMemoryDb::new())))
            .service(platform_versions),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/com.example.app/android")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn program_versions_reports_not_found_when_disabled() {
    let app = actix_test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(Db::InMemory(InMemoryDb::new())))
            .service(program_versions),
    )
    .await;
    let req = actix_test::TestRequest::get()
        .uri("/com.example.app")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn catalog_reports_not_found_when_disabled() {
    let app = actix_test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(Db::InMemory(InMemoryDb::new())))
            .service(catalog),
    )
    .await;
    let req = actix_test::TestRequest::get().uri("/").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
