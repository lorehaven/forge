use bytes::Bytes;
use gatehouse_service::ui::pages::home::render_home_page;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::request::Request;
use std::sync::Arc;

fn catalog() -> gatehouse_service::catalog::PermissionCatalog {
    let dir = std::env::temp_dir().join(format!("home-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("permissions.toml");
    std::fs::write(&path, "[services.gatehouse]\nactions = [\"read-users\"]\n").unwrap();
    gatehouse_service::catalog::PermissionCatalog::load_from(&path.to_string_lossy()).unwrap()
}

async fn body_text(resp: quench_http::response::Response) -> String {
    let collected = resp.into_hyper().into_body().collect().await.expect("body");
    String::from_utf8(collected.to_bytes().to_vec()).expect("utf8")
}

#[tokio::test]
async fn render_home_page_without_admin_omits_the_realm_section() {
    let resp = render_home_page(false, &[]);
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_text(resp).await;
    assert!(!html.is_empty());
    assert!(!html.contains("ui_home_group_realm"));
}

#[tokio::test]
async fn render_home_page_with_admin_includes_the_realm_section() {
    let resp = render_home_page(true, &[]);
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
            .provide(catalog())
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
            .provide(catalog())
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

#[tokio::test]
async fn cards_show_the_catalog_text_until_a_translation_applies() {
    let services = [gatehouse_service::services::ServiceLink {
        url: "https://vault.example.test".to_string(),
        title_key: "ui_service_vault_title".to_string(),
        desc_key: "ui_service_vault_desc".to_string(),
        label: "Vault".to_string(),
        description: Some("Keeps the things.".to_string()),
        card_class: "home-card-vault".to_string(),
    }];
    let html = body_text(render_home_page(false, &services)).await;
    assert!(html.contains("ui_service_vault_title"));
    // The pretty-printed markup puts the text on its own line.
    let squashed = html.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(squashed.contains("> Vault <"));
    assert!(squashed.contains("> Keeps the things. <"));
    assert!(html.contains("https://vault.example.test"));
}
