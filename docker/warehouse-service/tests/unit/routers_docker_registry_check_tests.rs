use actix_web::{App, test};
use warehouse_service::routers::docker::registry::check::{handle_get, handle_head};

#[actix_web::test]
async fn get_reports_ok_with_the_distribution_api_version_header() {
    let app = test::init_service(App::new().service(handle_get)).await;
    let req = test::TestRequest::get().uri("/").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("Docker-Distribution-API-Version")
            .unwrap(),
        "registry/2.0"
    );
}

#[actix_web::test]
async fn head_reports_the_same_as_get() {
    let app = test::init_service(App::new().service(handle_head)).await;
    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert!(
        resp.headers()
            .contains_key("Docker-Distribution-API-Version")
    );
}
