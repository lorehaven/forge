use crate::routers::ui::UiAuthenticated;
use crate::routers::ui::common::{render_page, ui_path};
use crate::routers::{models_dashboard_enabled, vllm_management_enabled};
use forge_ui::{PageHeader, empty_state, link_card, t};
use quench_http::prelude::{Response, get};
use quench_web::prelude::*;

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
    let mut cards = div().class("fui-grid");
    let mut any = false;

    if models_dashboard_enabled() {
        any = true;
        cards = cards.child(link_card(
            "cubes",
            &ui_path("/models/dashboard"),
            t("ui_service_models_dashboard_title"),
            Some(t("ui_service_models_dashboard_desc")),
        ));
    }

    if vllm_management_enabled() {
        any = true;
        cards = cards.child(link_card(
            "microchip",
            &ui_path("/vllm/manage"),
            t("ui_service_vllm_management_title"),
            Some(t("ui_service_vllm_management_desc")),
        ));
    }

    render_page(
        quench_http::http::StatusCode::OK,
        div()
            .class("fui-stack fui-stack-lg")
            .child(
                PageHeader::new("ui_home_title")
                    .subtitle_key("ui_home_group_services")
                    .build(),
            )
            .child(if any {
                cards
            } else {
                empty_state("circle-info", "ui_home_no_services", None)
            }),
    )
}

pub(super) fn register_routes() {
    let _ = home as fn(_) -> _;
    let _ = home_slash as fn(_) -> _;
}
