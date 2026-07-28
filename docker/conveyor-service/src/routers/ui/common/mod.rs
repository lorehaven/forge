//! Page shell, shared with the rest of the estate: same builder, same theme,
//! same header, same generated stylesheet layout.

use actix_web::{HttpResponse, Responder, get, http::header::ContentType, web};
pub use quench_starter::actix::routers::ui::{
    is_ui_authenticated, ui_asset_path, ui_login_redirect, ui_login_redirect_for, ui_path,
};
use quench_web::prelude::*;
use std::sync::LazyLock;

pub mod css;
pub mod format;

const SUPPORTED_LOCALES: [&str; 5] = ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"];

fn supported_locales() -> Vec<String> {
    SUPPORTED_LOCALES.iter().map(|s| s.to_string()).collect()
}

fn shell(title_key: &str, show_home: bool) -> AppShell {
    css::ensure_conveyor_css();

    AppShellBuilder::new()
        .title("Conveyor")
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(title_key, show_home))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/conveyor.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
}

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| shell("ui_header_home", false));

fn ui_header(title_key: &str, show_home: bool) -> Element {
    header()
        .child(
            div()
                .class("left-panel")
                .child(h2().attr("data-i18n", title_key)),
        )
        .child(
            div()
                .class("right-panel")
                .child(locale_switch(Some(supported_locales()), None))
                .child_opt(show_home.then(|| {
                    a().attr("href", ui_path("/home"))
                        .class("button")
                        .attr("data-i18n", "ui_home_button")
                }))
                .child(
                    a().attr("href", ui_path("/logout"))
                        .class("button")
                        .attr("data-i18n", "ui_logout"),
                ),
        )
}

/// Writes `dist/assets` before the first request. Left to the first page
/// render, a request for the stylesheet that arrives earlier is answered from
/// whatever the previous deployment left on disk.
pub fn ensure_assets() {
    LazyLock::force(&UI_SHELL_HOME);
}

#[get("/assets/{path:.*}")]
pub async fn assets(path: web::Path<String>) -> impl Responder {
    quench_starter::actix::routers::ui::serve_assets(path, "dist/assets").await
}

/// The estate-wide way to show a run, job or step status.
///
/// Both classes are emitted: `.status` carries the shape, `.status-<state>` the
/// colour. The label is a translation key rather than the raw status string, so
/// the pill reads in the viewer's language.
pub fn status_pill(status: crate::domain::Status) -> Element {
    span()
        .class(format!("status status-{status}"))
        .attr("data-i18n", format!("ui_status_{status}"))
}

pub fn render_page(
    mut builder: actix_web::HttpResponseBuilder,
    content: Element,
    page_kind: UiPageKind,
) -> HttpResponse {
    let shell = match page_kind {
        UiPageKind::Home => &*UI_SHELL_HOME,
    };
    builder
        .content_type(ContentType::html())
        .body(shell.page(div().class("page").child(content)))
}

pub enum UiPageKind {
    Home,
}
