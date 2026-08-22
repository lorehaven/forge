use actix_web::HttpResponse;
use actix_web::body::to_bytes;
use gatehouse_service::ui::common::{UiPageKind, ensure_assets, render_page, supported_locales};
use quench_web::prelude::*;

#[test]
fn supported_locales_lists_every_configured_locale() {
    let locales = supported_locales();
    assert_eq!(locales.len(), 5);
    assert!(locales.contains(&"en-US".to_string()));
    assert!(locales.contains(&"pl-PL".to_string()));
}

#[tokio::test]
async fn render_page_wraps_content_in_html_for_every_page_kind() {
    for kind in [
        UiPageKind::Home,
        UiPageKind::Auth,
        UiPageKind::Admin,
        UiPageKind::Account,
    ] {
        let resp = render_page(HttpResponse::Ok(), div().text("marker-content"), kind);
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(resp.into_body()).await.expect("body");
        let html = String::from_utf8(body.to_vec()).expect("utf8");
        assert!(html.contains("marker-content"));
    }
}

#[test]
fn ensure_assets_does_not_panic() {
    ensure_assets();
}
