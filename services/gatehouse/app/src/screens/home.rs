//! The services dashboard: a card for every service this deployment offers, and the realm's own section for an
//! administrator.

use gatehouse_core::{Dashboard, ServiceCard};
use quench_ui::prelude::*;
use quench_ui::router::{RouteContext, link};

use super::{Env, waiting};
use crate::live::{Request, State};

pub(crate) fn home(env: &Env, state: &State, _ctx: &RouteContext) -> View {
    state.dashboard.set(None);
    (env.ask)(Request::Dashboard);
    let (env, dashboard) = (env.clone(), state.dashboard.clone());
    View::dynamic(move || {
        dashboard
            .get()
            .map_or_else(|| waiting(&env), |dashboard| page(&env, &dashboard))
    })
}

fn page(env: &Env, dashboard: &Dashboard) -> View {
    let mut sections = div()
        .class("home-sections")
        .attr("id", "dashboard-sections");
    if dashboard.services.is_empty() {
        sections = sections.child(
            p().attr("id", "no-services")
                .class("fui-empty")
                .text(env.t.t("ui_home_no_services")),
        );
    } else {
        let mut cards = div()
            .class("fui-card-grid home-grid")
            .attr("id", "services");
        for service in &dashboard.services {
            cards = cards.child(service_card(env, service));
        }
        sections = sections.child(
            div()
                .class("home-section")
                .child(
                    h3().class("home-section-title")
                        .text(env.t.t("ui_home_group_services")),
                )
                .child(cards),
        );
    }

    // The realm itself, not a service: its own section.
    if dashboard.admin {
        let card = link(&env.path("/admin/users"))
            .class("fui-link-card home-card home-card-gatehouse")
            .attr("id", "realm-users")
            .child(card_icon(&env.path("/assets/icons/id-card.png")))
            .child(
                div()
                    .class("fui-link-card-body home-card-body")
                    .child(
                        div()
                            .class("fui-link-card-title home-card-title")
                            .text(env.t.t("ui_admin_users_title")),
                    )
                    .child(
                        div()
                            .class("fui-link-card-desc home-card-desc")
                            .text(env.t.t("ui_admin_users_desc")),
                    ),
            )
            .child(arrow());
        sections = sections.child(
            div()
                .class("home-section")
                .child(
                    h3().class("home-section-title")
                        .text(env.t.t("ui_home_group_realm")),
                )
                .child(div().class("fui-card-grid home-grid").child(card)),
        );
    }

    div()
        .class("home-container")
        .child(
            div()
                .class("home-header")
                .child(h3().text(env.t.t("ui_home_title")))
                .child(p().class("home-subtitle").text(env.t.t("ui_home_subtitle"))),
        )
        .child(sections)
        .into_view()
}

/// A card links to the service's own UI (a page of its own, not a route of this app). The words are the translation, else
/// the service's own label and description.
fn service_card(env: &Env, service: &ServiceCard) -> Element {
    a().attr("href", service.url.clone())
        .attr("data-service", service.label.clone())
        .class(format!("fui-link-card home-card {}", service.card_class))
        .child(
            gatehouse_core::model::ICONS
                .contains(&service.slug())
                .then(|| card_icon(&env.path(&format!("/assets/icons/{}.png", service.slug())))),
        )
        .child(
            div()
                .class("fui-link-card-body home-card-body")
                .child(
                    div()
                        .class("fui-link-card-title home-card-title")
                        .text(env.tr_or(&service.title_key, &service.label)),
                )
                .child(
                    div()
                        .class("fui-link-card-desc home-card-desc")
                        .text(env.tr_or(
                            &service.desc_key,
                            service.description.as_deref().unwrap_or_default(),
                        )),
                ),
        )
        .child(arrow())
}

/// The service's picture on its card.
fn card_icon(src: &str) -> Element {
    img()
        .class("fui-link-card-img home-card-icon")
        .attr("src", src.to_string())
        .attr("alt", "")
}

/// The arrow at the end of a card.
fn arrow() -> Element {
    div()
        .class("fui-link-card-arrow home-card-arrow")
        .child(forge_shell::icon("arrow-right"))
}
