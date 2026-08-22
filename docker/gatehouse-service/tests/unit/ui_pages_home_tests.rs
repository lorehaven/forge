use actix_web::body::to_bytes;
use actix_web::{App, HttpResponse, test as actix_test, web};
use gatehouse_service::ui::pages::home::{home, home_slash, render_home_page};
use quench_auth::prelude::JwtConfig;

async fn body_text(resp: HttpResponse) -> String {
    let body = to_bytes(resp.into_body()).await.expect("body");
    String::from_utf8(body.to_vec()).expect("utf8")
}

#[tokio::test]
async fn render_home_page_without_admin_omits_the_realm_section() {
    let resp = render_home_page(false);
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let html = body_text(resp).await;
    assert!(!html.is_empty());
    assert!(!html.contains("ui_home_group_realm"));
}

#[tokio::test]
async fn render_home_page_with_admin_includes_the_realm_section() {
    let resp = render_home_page(true);
    let html = body_text(resp).await;
    assert!(html.contains("ui_home_group_realm"));
    assert!(html.contains("ui_admin_users_title"));
}

#[actix_web::test]
async fn home_renders_when_auth_is_disabled() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .service(home),
    )
    .await;
    let req = actix_test::TestRequest::get().uri("/home").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success() || resp.status().is_redirection());
}

#[actix_web::test]
async fn home_slash_renders_when_auth_is_disabled() {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(JwtConfig::for_tests()))
            .service(home_slash),
    )
    .await;
    let req = actix_test::TestRequest::get().uri("/home/").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success() || resp.status().is_redirection());
}
