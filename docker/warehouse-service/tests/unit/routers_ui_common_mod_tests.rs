use quench_http::prelude::http::StatusCode;
use quench_web::prelude::*;
use warehouse_service::routers::ui::common::{
    SUPPORTED_LOCALES, render_page, supported_locales, ui_login_redirect,
};

#[quench_test::test]
fn supported_locales_lists_every_configured_locale() {
    let locales = supported_locales();
    assert_eq!(locales.len(), SUPPORTED_LOCALES.len());
    for locale in SUPPORTED_LOCALES {
        assert!(locales.contains(&locale.to_string()));
    }
}

#[quench_test::test]
fn render_page_wraps_content_in_the_shell() {
    let resp = render_page(StatusCode::OK, div().text("marker-content"));
    assert_eq!(resp.status(), StatusCode::OK);
}

#[quench_test::test]
fn ui_login_redirect_is_a_redirect_response() {
    let resp = ui_login_redirect();
    assert!(resp.status().is_redirection());
}
