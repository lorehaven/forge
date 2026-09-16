use bytes::Bytes;
use gatehouse_service::ui::pages::home::render_home_page;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::request::Request;
use std::sync::Arc;

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8(collected.to_bytes().to_vec()).expect("utf8")
}

#[tokio::test]
async fn render_home_page_without_admin_omits_the_realm_section() {
    let resp = render_home_page(false);
    assert_eq!(resp.status(), StatusCode::OK);
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

#[tokio::test]
async fn home_renders_when_auth_is_disabled() {
    gatehouse_service::ui::register_routes();
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(JwtConfig::for_tests())
            .build()
            .await
            .unwrap(),
    );
    let app = quench_starter::http::discover_and_mount("/");

    let req = Request::new(
        Method::GET,
        "/ui/home".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container,
    );
    let resp = app.call(req).await;
    assert!(resp.status().is_success() || resp.status().is_redirection());
}

#[tokio::test]
async fn home_slash_renders_when_auth_is_disabled() {
    gatehouse_service::ui::register_routes();
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(JwtConfig::for_tests())
            .build()
            .await
            .unwrap(),
    );
    let app = quench_starter::http::discover_and_mount("/");

    let req = Request::new(
        Method::GET,
        "/ui/home/".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container,
    );
    let resp = app.call(req).await;
    assert!(resp.status().is_success() || resp.status().is_redirection());
}
