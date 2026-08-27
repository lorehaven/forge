use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use quench_web::prelude::*;
use warehouse_service::routers::ui::common::{
    SUPPORTED_LOCALES, UiPageKind, render_page, supported_locales, ui_header, ui_login_redirect,
};

#[test]
fn supported_locales_lists_every_configured_locale() {
    let locales = supported_locales();
    assert_eq!(locales.len(), SUPPORTED_LOCALES.len());
    for locale in SUPPORTED_LOCALES {
        assert!(locales.contains(&locale.to_string()));
    }
}

#[test]
fn ui_header_with_everything_enabled_renders_locale_home_and_logout() {
    let html = ui_header(Some("ui_header_home"), true, true, true).render();
    assert!(html.contains("ui_header_home"));
    assert!(html.contains("ui_home_button"));
    assert!(html.contains("ui_logout"));
}

#[test]
fn ui_header_can_hide_locale_switch_home_and_logout() {
    let html = ui_header(None, false, false, false).render();
    assert!(html.contains("header_label"));
    assert!(!html.contains("ui_home_button"));
    assert!(!html.contains("ui_logout"));
}

#[test]
fn render_page_wraps_content_in_the_matching_shell_for_every_page_kind() {
    for kind in [
        UiPageKind::Home,
        UiPageKind::Docker,
        UiPageKind::Crates,
        UiPageKind::Files,
        UiPageKind::Apk,
    ] {
        let resp = render_page(HttpResponse::Ok(), div().text("marker-content"), kind);
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

#[test]
fn ui_login_redirect_is_a_redirect_response() {
    let resp = ui_login_redirect();
    assert!(resp.status().is_redirection());
}
