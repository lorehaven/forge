use crate::routers::ui::UiAuthenticated;
use crate::routers::ui::common::{UiPageKind, render_page, ui_path};
use crate::routers::{models_dashboard_enabled, vllm_management_enabled};
use quench_http::prelude::{Response, get};
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;

#[get("/ui/home")]
pub(super) async fn home(UiAuthenticated(authenticated): UiAuthenticated) -> Response {
    if !authenticated {
        return crate::routers::ui::common::ui_login_redirect();
    }
    render_home_page()
}

#[get("/ui/home/")]
pub(super) async fn home_slash(UiAuthenticated(authenticated): UiAuthenticated) -> Response {
    if !authenticated {
        return crate::routers::ui::common::ui_login_redirect();
    }
    render_home_page()
}

fn render_home_page() -> Response {
    let mut service_cards = div().class("home-grid");
    let mut has_service_cards = false;

    if models_dashboard_enabled() {
        has_service_cards = true;
        service_cards = service_cards.child(service_card(
            &ui_path("/models/dashboard"),
            "ui_service_models_dashboard_title",
            "ui_service_models_dashboard_desc",
            "home-card-models-dashboard",
        ));
    }

    if vllm_management_enabled() {
        has_service_cards = true;
        service_cards = service_cards.child(service_card(
            &ui_path("/vllm/manage"),
            "ui_service_vllm_management_title",
            "ui_service_vllm_management_desc",
            "home-card-vllm-management",
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
        http::StatusCode::OK,
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
        .class(format!("home-card {extra_class}"))
        .child(
            div()
                .class("home-card-body")
                .child(div().class("home-card-title").attr("data-i18n", title_key))
                .child(div().class("home-card-desc").attr("data-i18n", desc_key)),
        )
        .child(div().class("home-card-arrow").text("→"))
}

pub(super) fn register_routes() {
    let _ = home as fn(_) -> _;
    let _ = home_slash as fn(_) -> _;
}
