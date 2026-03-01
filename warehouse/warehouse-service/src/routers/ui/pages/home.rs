use crate::routers::files::list_storage_infos;
use crate::routers::ui::common::{UiPageKind, render_page};
use crate::routers::{crates_enabled, docker_enabled, files_enabled};
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
    let mut service_cards = div().class("home-grid");
    let mut file_cards = div().class("home-grid");
    let mut has_service_cards = false;
    let mut has_file_cards = false;

    if docker_enabled() {
        has_service_cards = true;
        service_cards = service_cards.child(service_card(
            "/ui/docker/catalog",
            "ui_service_docker_title",
            "ui_service_docker_desc",
            "home-card-docker",
        ));
    }

    if crates_enabled() {
        has_service_cards = true;
        service_cards = service_cards.child(service_card(
            "/ui/crates/catalog",
            "ui_service_crates_title",
            "ui_service_crates_desc",
            "home-card-crates",
        ));
    }

    if files_enabled() {
        has_file_cards = true;
        for storage in list_storage_infos() {
            file_cards = file_cards.child(
                a().attr(
                    "href",
                    &format!("/ui/files/catalog?storage={}", storage.name),
                )
                .class("home-card home-card-files")
                .child(
                    div()
                        .class("home-card-body")
                        .child(
                            div()
                                .class("home-card-title")
                                .text(&format!("Files: {}", storage.name)),
                        )
                        .child(
                            div()
                                .class("home-card-desc")
                                .text(&format!("Root: {}", storage.root)),
                        ),
                )
                .child(div().class("home-card-arrow").text("→")),
            );
        }
    }

    let mut sections = div().class("home-sections");

    if has_service_cards {
        sections = sections.child(
            div()
                .class("home-section")
                .child(
                    h3()
                        .class("home-section-title")
                        .attr("data-i18n", "ui_home_group_services"),
                )
                .child(service_cards),
        );
    }

    if has_file_cards {
        sections = sections.child(
            div()
                .class("home-section")
                .child(
                    h3()
                        .class("home-section-title")
                        .attr("data-i18n", "ui_home_group_files"),
                )
                .child(file_cards),
        );
    }

    if !has_service_cards && !has_file_cards {
        sections = sections.child(
            div()
                .class("empty")
                .attr("data-i18n", "ui_home_no_services"),
        );
    }

    render_page(
        HttpResponse::Ok(),
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
