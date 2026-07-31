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
    let admin = is_admin(&req, &config).await;
    handle_home(req, config, move || render_home_page(admin)).await
}

#[get("/home/")]
pub(super) async fn home_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    let admin = is_admin(&req, &config).await;
    handle_home(req, config, move || render_home_page(admin)).await
}

/// Whether to offer the realm administration link.
///
/// Cosmetic only - the admin pages check for themselves, and so does the API
/// behind them. But a visible control that answers 403 is worse than no control.
async fn is_admin(req: &actix_web::HttpRequest, config: &JwtConfig) -> bool {
    quench_auth::actix::routers::ui::get_user_from_req(req, config)
        .await
        .is_some_and(|claims| claims.has_role("admin"))
}

fn render_home_page(admin: bool) -> HttpResponse {
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

    // The realm itself, not a service: rendered as its own section so it does not
    // look like another destination in the estate.
    if admin {
        sections = sections.child(
            div()
                .class("home-section")
                .child(
                    h3().class("home-section-title")
                        .attr("data-i18n", "ui_home_group_realm"),
                )
                .child(div().class("home-grid").child(service_card(
                    &crate::ui::common::ui_path("/admin/users"),
                    "ui_admin_users_title",
                    "ui_admin_users_desc",
                    "home-card-gatehouse",
                ))),
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
