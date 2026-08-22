use actix_web::http::StatusCode;
use actix_web::test as actix_test;
use warehouse_service::routers::files::ops::delete::handle;

#[actix_web::test]
async fn handle_reports_not_found_when_file_storage_is_disabled() {
    let app = actix_test::init_service(actix_web::App::new().service(handle)).await;
    let req = actix_test::TestRequest::delete()
        .uri("/artifacts/file?path=a.txt")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
