use crate::routers::ui::common::{UiPageKind, render_page};
use crate::routers::{crates_enabled, docker_enabled};
use actix_web::{HttpResponse, Responder, get};
use quench::prelude::*;

#[get("/home")]
pub(super) async fn home() -> impl Responder {
    render_home_page()
}

#[get("/home/")]
pub(super) async fn home_slash() -> impl Responder {
    render_home_page()
}

fn render_home_page() -> HttpResponse {
    let mut cards = div().class("home-grid");

    if docker_enabled() {
        cards = cards.child(service_card(
            "/ui/docker/catalog",
            "ui_service_docker_title",
            "ui_service_docker_desc",
            "home-card-docker",
        ));
    }

    if crates_enabled() {
        cards = cards.child(service_card(
            "/ui/crates/catalog",
            "ui_service_crates_title",
            "ui_service_crates_desc",
            "home-card-crates",
        ));
    }

    // If somehow neither feature is on, show a placeholder
    if !docker_enabled() && !crates_enabled() {
        cards = cards.child(
            div()
                .class("empty")
                .attr("data-i18n", "ui_home_no_services"),
        );
    }

    render_page(
        HttpResponse::Ok(),
        content().class("container-fluid py-4").child(
            div()
                .class("home-layout")
                .child(
                    div()
                        .class("home-header")
                        .child(h3().attr("data-i18n", "ui_home_title"))
                        .child(
                            p().class("home-subtitle")
                                .attr("data-i18n", "ui_home_subtitle"),
                        ),
                )
                .child(cards),
        ),
        UiPageKind::Home,
    )
}

fn service_card(href: &str, title_key: &str, desc_key: &str, extra_class: &str) -> Element {
    a().attr("href", href)
        .class(&format!("home-card {extra_class}"))
        .child(
            div()
                .class("home-card-body")
                .child(div().class("home-card-title").attr("data-i18n", title_key))
                .child(div().class("home-card-desc").attr("data-i18n", desc_key)),
        )
        .child(div().class("home-card-arrow").text("→"))
}
