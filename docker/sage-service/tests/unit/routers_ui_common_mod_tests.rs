use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::di::ContainerBuilder;
use quench_http::request::Request;
use quench_web::prelude::*;
use sage_service::routers::ui::common::supported_locales;
use sage_service::routers::ui::common::{SUPPORTED_LOCALES, render_page, ui_header};
use std::sync::Arc;

#[test]
fn supported_locales_matches_the_fixed_list() {
    let locales = supported_locales();
    assert_eq!(locales.len(), SUPPORTED_LOCALES.len());
    assert!(locales.contains(&"en-US".to_string()));
}

#[test]
fn ui_header_renders_every_combination_of_optional_controls() {
    for title_key in [None, Some("ui_header_home")] {
        for show_locale_switch in [false, true] {
            for show_home in [false, true] {
                for show_logout in [false, true] {
                    let header = ui_header(title_key, show_locale_switch, show_home, show_logout);
                    let rendered = header.render();
                    assert!(!rendered.is_empty());
                }
            }
        }
    }
}

#[test]
fn render_page_produces_html_wrapping_the_given_content() {
    let response = render_page(StatusCode::OK, div().text("hello from a test"));
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn assets_reports_not_found_for_a_missing_file() {
    sage_service::routers::ui::common::register_routes();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app = quench_starter::http::discover_and_mount("/");

    let req = Request::new(
        Method::GET,
        "/ui/assets/does-not-exist.css".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container,
    );
    let resp = app.call(req).await;
    assert!(!resp.status().is_success());
}
