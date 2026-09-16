use crate::routers::ui::common::{PageAuth, UiPageKind, render_page, ui_login_redirect, ui_path};
use crate::routers::{artifacts_enabled, crates_enabled, docker_enabled, files_enabled};
use quench_http::prelude::{Response, get, http::StatusCode};
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;

#[get("/ui/home")]
pub async fn home(PageAuth(authenticated): PageAuth) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }
    render_home_page()
}

#[get("/ui/home/")]
pub async fn home_slash(PageAuth(authenticated): PageAuth) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }
    render_home_page()
}

/// Ported from `quench_starter::actix::routers::ui::pages::home::service_card` - never split out under `http`.
fn service_card(href: &str, title_key: &str, desc_key: &str, extra_class: &str) -> Element {
    a().attr("href", href)
        .class(format!("home-card {extra_class}"))
        .child(
            div()
                .class("home-card-body")
                .child(div().class("home-card-title").attr("data-i18n", title_key))
                .child(div().class("home-card-desc").attr("data-i18n", desc_key)),
        )
        .child(div().class("home-card-arrow").text("→"))
}

pub fn render_home_page() -> Response {
    let mut service_cards = div().class("home-grid");
    let mut has_service_cards = false;

    if docker_enabled() {
        has_service_cards = true;
        service_cards = service_cards.child(service_card(
            &ui_path("/docker/catalog"),
            "ui_service_docker_title",
            "ui_service_docker_desc",
            "home-card-docker",
        ));
    }

    if crates_enabled() {
        has_service_cards = true;
        service_cards = service_cards.child(service_card(
            &ui_path("/crates/catalog"),
            "ui_service_crates_title",
            "ui_service_crates_desc",
            "home-card-crates",
        ));
    }

    if files_enabled() {
        has_service_cards = true;
        service_cards = service_cards.child(service_card(
            &ui_path("/files/storages"),
            "ui_service_files_title",
            "ui_service_files_desc",
            "home-card-files",
        ));
    }

    if artifacts_enabled() {
        has_service_cards = true;
        service_cards = service_cards.child(service_card(
            &ui_path("/artifacts/catalog"),
            "ui_service_artifacts_title",
            "ui_service_artifacts_desc",
            "home-card-artifacts",
        ));
    }

    let mut sections = div().class("home-sections");

    if has_service_cards {
        sections = sections.child(
            div()
                .class("home-section")
                .child(
                    h3().class("home-section-title")
                        .attr("data-i18n", "ui_home_group_services"),
                )
                .child(service_cards),
        );
    }

    if !has_service_cards {
        sections = sections.child(empty_state("ui_home_no_services"));
    }

    render_page(
        StatusCode::OK,
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(
                    div()
                        .class("home-header")
                        .child(h3().attr("data-i18n", "ui_home_title")),
                )
                .child(sections),
        ),
        UiPageKind::Home,
    )
}

pub fn register_routes() {
    let _ = home as fn(_) -> _;
    let _ = home_slash as fn(_) -> _;
}
