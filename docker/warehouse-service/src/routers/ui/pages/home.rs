use crate::routers::files::list_storage_infos;
use crate::routers::ui::common::{UiPageKind, render_page, ui_path};
use crate::routers::{crates_enabled, docker_enabled, files_enabled};
use actix_web::{HttpResponse, Responder, get, web};
use quench_srv::actix::routers::ui::pages::home::{handle_home, service_card};
use quench_srv::prelude::jwt::JwtConfig;
use quench_web::prelude::*;

#[get("/home")]
pub(super) async fn home(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    handle_home(req, config, render_home_page).await
}

#[get("/home/")]
pub(super) async fn home_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    handle_home(req, config, render_home_page).await
}

fn render_home_page() -> HttpResponse {
    let mut service_cards = div().class("home-grid");
    let mut file_cards = div().class("home-grid");
    let mut has_service_cards = false;
    let mut has_file_cards = false;

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
        has_file_cards = true;
        for storage in list_storage_infos() {
            file_cards = file_cards.child(
                a().attr(
                    "href",
                    &format!("{}?storage={}", ui_path("/files/catalog"), storage.name),
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
                    h3().class("home-section-title")
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
                    h3().class("home-section-title")
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
