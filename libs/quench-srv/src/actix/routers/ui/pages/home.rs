use crate::actix::domain::jwt::JwtConfig;
use crate::actix::routers::ui::{is_ui_authenticated, ui_login_redirect};
use actix_web::{HttpResponse, web};
use quench_web::prelude::*;

pub async fn handle_home<F>(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    render_fn: F,
) -> HttpResponse
where
    F: FnOnce() -> HttpResponse,
{
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }
    render_fn()
}

pub fn service_card(href: &str, title_key: &str, desc_key: &str, extra_class: &str) -> Element {
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
