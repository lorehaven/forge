//! The estate's front door: every service this deployment offers, in one place.

use crate::services::enabled_services;
use crate::ui::common::{UiPageKind, render_page};
use async_trait::async_trait;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::routers::ui::{get_user_from_req, is_ui_authenticated};
use quench_http::prelude::{FromRequest, HttpError, Request, Response, get};
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;

/// Session validity, plus (cosmetic only) whether to show the admin link.
pub struct HomeAuth {
    authenticated: bool,
    admin: bool,
}

#[async_trait]
impl FromRequest for HomeAuth {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self {
                authenticated: false,
                admin: false,
            });
        };
        let authenticated = is_ui_authenticated(req, &config).await;
        let admin = get_user_from_req(req, &config)
            .await
            .is_some_and(|claims| claims.has_role("admin"));
        Ok(Self {
            authenticated,
            admin,
        })
    }
}

#[get("/ui/home")]
pub async fn home(auth: HomeAuth) -> Response {
    if !auth.authenticated {
        return crate::ui::pages::auth::login_redirect();
    }
    render_home_page(auth.admin)
}

#[get("/ui/home/")]
pub async fn home_slash(auth: HomeAuth) -> Response {
    if !auth.authenticated {
        return crate::ui::pages::auth::login_redirect();
    }
    render_home_page(auth.admin)
}

pub fn render_home_page(admin: bool) -> Response {
    let services = enabled_services();

    let mut sections = div().class("home-sections");

    if services.is_empty() {
        sections = sections.child(empty_state("ui_home_no_services"));
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

    // The realm itself, not a service - its own section.
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
        http::StatusCode::OK,
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

pub fn register_routes() {
    let _ = home as fn(_) -> _;
    let _ = home_slash as fn(_) -> _;
}
