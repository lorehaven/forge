//! The estate's front door: every service this deployment offers, in one place.

use crate::services::enabled_services;
use crate::ui::common::{UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, web};
use quench_auth::prelude::JwtConfig;
use quench_starter::actix::routers::ui::pages::home::{handle_home, service_card};
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
    let services = enabled_services();

    let mut sections = div().class("home-sections");

    if services.is_empty() {
        sections = sections.child(
            div()
                .class("empty")
                .attr("data-i18n", "ui_home_no_services"),
        );
    } else {
        let mut cards = div().class("home-grid");
        for service in &services {
            cards = cards.child(service_card(
                &service.url,
                service.title_key,
                service.desc_key,
                service.card_class,
            ));
        }

        sections = sections.child(
            div()
                .class("home-section")
                .child(
                    h3().class("home-section-title")
                        .attr("data-i18n", "ui_home_group_services"),
                )
                .child(cards),
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
                        .child(h3().attr("data-i18n", "ui_home_title"))
                        .child(
                            p().class("home-subtitle")
                                .attr("data-i18n", "ui_home_subtitle"),
                        ),
                )
                .child(sections),
        ),
        UiPageKind::Home,
    )
}
