//! `supported_locales`/`ui_header`/`ui_header_split` in
//! `routers/ui/common/mod.rs`, plus a light smoke test of the `assets`
//! handler's 404 path (no `dist/assets` fixture is set up for a real file).

use actix_web::App;
use actix_web::test as actix_test;
use switchboard_service::routers::ui::common::{
    assets, supported_locales, ui_header, ui_header_split,
};

#[test]
fn supported_locales_lists_all_five_configured_locales() {
    let locales = supported_locales();
    assert_eq!(locales.len(), 5);
    assert!(locales.contains(&"en-US".to_string()));
    assert!(locales.contains(&"pl-PL".to_string()));
}

#[test]
fn ui_header_renders_without_panicking_with_every_flag_combination() {
    for show_locale in [true, false] {
        for show_home in [true, false] {
            for show_logout in [true, false] {
                let element = ui_header(None, show_locale, show_home, show_logout);
                let html = element.render();
                assert!(!html.is_empty());
                if show_home {
                    assert!(html.contains("ui_home_button"));
                }
                if show_logout {
                    assert!(html.contains("ui_logout"));
                }
            }
        }
    }
}

#[test]
fn ui_header_uses_the_given_title_key_or_falls_back_to_the_default() {
    let with_title = ui_header(Some("ui_header_home"), false, false, false).render();
    assert!(with_title.contains("ui_header_home"));

    let default_title = ui_header(None, false, false, false).render();
    assert!(default_title.contains("header_label"));
}

#[test]
fn ui_header_split_renders_both_title_keys() {
    let html =
        ui_header_split("ui_header_dashboard", "ui_header_models", true, true, true).render();
    assert!(html.contains("ui_header_dashboard"));
    assert!(html.contains("ui_header_models"));
    assert!(html.contains("header-split"));
}

#[actix_web::test]
async fn assets_returns_not_found_for_a_nonexistent_asset() {
    let app = actix_test::init_service(App::new().service(assets)).await;
    let req = actix_test::TestRequest::get()
        .uri("/assets/does-not-exist.css")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(!resp.status().is_success());
}
