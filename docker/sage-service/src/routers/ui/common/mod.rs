use actix_web::{HttpResponse, Responder, get, http::header::ContentType, web};
pub use quench_starter::actix::routers::ui::{
    is_ui_authenticated, ui_asset_path, ui_login_redirect, ui_path,
};
use quench_web::prelude::*;
use std::sync::LazyLock;

mod css;
pub mod format;

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_sage_css();

    AppShellBuilder::new()
        .title("Sage")
        .supported_locales(
            ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"].iter().map(|s| s.to_string()).collect(),
        )
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(Some("ui_header_home"), false, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/sage.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_AUTH: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_sage_css();

    AppShellBuilder::new()
        .title("Sage")
        .supported_locales(
            ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"].iter().map(|s| s.to_string()).collect(),
        )
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(None, false, false))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/sage.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

fn ui_header(title_key: Option<&str>, show_home: bool, show_logout: bool) -> Element {
    let title = match title_key {
        Some(key) => h2().attr("data-i18n", key),
        None => h2().attr("data-i18n", "header_label"),
    };

    header()
        .child(div().class("left-panel").child(title))
        .child(
            div()
                .class("right-panel")
                .child_opt(show_home.then(|| {
                    a().attr("href", ui_path("/home"))
                        .class("button")
                        .attr("data-i18n", "ui_home_button")
                }))
                .child_opt(show_logout.then(|| {
                    a().attr("href", ui_path("/logout"))
                        .class("button")
                        .attr("data-i18n", "ui_logout")
                })),
        )
}

#[get("/assets/{path:.*}")]
pub async fn assets(path: web::Path<String>) -> impl Responder {
    quench_starter::actix::routers::ui::serve_assets(path, "dist/assets").await
}

pub(super) fn render_page(
    mut builder: actix_web::HttpResponseBuilder,
    content: Element,
    page_kind: UiPageKind,
) -> HttpResponse {
    let shell = match page_kind {
        UiPageKind::Home => &*UI_SHELL_HOME,
        UiPageKind::Auth => &*UI_SHELL_AUTH,
    };
    builder
        .content_type(ContentType::html())
        .body(shell.page(div().class("page").child(content)))
}

pub(super) enum UiPageKind {
    Home,
    Auth,
}
