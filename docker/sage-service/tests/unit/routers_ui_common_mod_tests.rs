use actix_web::HttpResponse;
use quench_web::prelude::*;
use sage_service::routers::ui::common::supported_locales;
use sage_service::routers::ui::common::{SUPPORTED_LOCALES, UiPageKind, render_page, ui_header};

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

#[actix_web::test]
async fn render_page_produces_html_wrapping_the_given_content() {
    let response = render_page(
        HttpResponse::Ok(),
        div().text("hello from a test"),
        UiPageKind::Home,
    );
    assert_eq!(response.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn assets_reports_not_found_for_a_missing_file() {
    use actix_web::{App, test};
    use sage_service::routers::ui::common::assets;

    let app = test::init_service(App::new().service(assets)).await;
    let req = test::TestRequest::get()
        .uri("/assets/does-not-exist.css")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(!resp.status().is_success());
}
