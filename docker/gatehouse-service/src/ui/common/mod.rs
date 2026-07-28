//! Page shell, shared with the rest of the estate: same builder, same theme,
//! same header, same generated stylesheet layout.

use actix_web::{HttpResponse, Responder, get, http::header::ContentType, web};
pub use quench_starter::actix::routers::ui::{is_ui_authenticated, ui_asset_path, ui_path};
use quench_web::prelude::*;
use std::sync::LazyLock;

mod css;

const SUPPORTED_LOCALES: [&str; 5] = ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"];

pub(super) fn supported_locales() -> Vec<String> {
    SUPPORTED_LOCALES.iter().map(|s| s.to_string()).collect()
}

fn shell(header: Option<Element>) -> AppShell {
    css::ensure_gatehouse_css();

    let mut builder = AppShellBuilder::new()
        .title("Gatehouse")
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/gatehouse.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""));

    builder = match header {
        Some(header) => builder.header(header),
        None => builder.with_header(false),
    };

    builder.build()
}

static UI_SHELL_HOME: LazyLock<AppShell> =
    LazyLock::new(|| shell(Some(ui_header("ui_home_title", true, false, true))));

// The login page carries its own bar on the card, so the shell has no top
// panel: there is nowhere to go home to and nothing to log out of either.
static UI_SHELL_AUTH: LazyLock<AppShell> = LazyLock::new(|| shell(None));

fn ui_header(
    title_key: &str,
    show_locale_switch: bool,
    show_home: bool,
    show_logout: bool,
) -> Element {
    let title = h2().attr("data-i18n", title_key);

    header()
        .child(div().class("left-panel").child(title))
        .child(
            div()
                .class("right-panel")
                .child_opt(
                    show_locale_switch.then(|| locale_switch(Some(supported_locales()), None)),
                )
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

/// Writes `dist/assets` before the first request. Left to the first page
/// render, a request for the stylesheet that arrives earlier is answered from
/// whatever the previous deployment left on disk.
pub fn ensure_assets() {
    LazyLock::force(&UI_SHELL_HOME);
    LazyLock::force(&UI_SHELL_AUTH);
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
