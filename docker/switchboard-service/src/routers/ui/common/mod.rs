use actix_web::{HttpResponse, Responder, get, http::header::ContentType, web};
pub use quench_srv::actix::routers::ui::{is_ui_authenticated, ui_asset_path, ui_path};
use quench_web::prelude::*;
use std::sync::LazyLock;

mod css;

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_switchboard_css();

    AppShellBuilder::new()
        .title("Switchboard")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(Some("ui_header_home"), true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/switchboard.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_MODELS_DASHBOARD: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_switchboard_css();

    AppShellBuilder::new()
        .title("Switchboard")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header_split(
            "ui_header_dashboard",
            "ui_header_models",
            true,
            true,
        ))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/switchboard.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_VLLM_MANAGEMENT: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_switchboard_css();

    AppShellBuilder::new()
        .title("Switchboard")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header_split(
            "ui_header_dashboard",
            "ui_header_vllm",
            true,
            true,
        ))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/switchboard.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_AUTH: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_switchboard_css();

    AppShellBuilder::new()
        .title("Switchboard")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(None, false, false))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/switchboard.css"),
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

fn ui_header_split(
    title1_key: &str,
    title2_key: &str,
    show_home: bool,
    show_logout: bool,
) -> Element {
    let title = div()
        .class("header-split")
        .child(h2().attr("data-i18n", title1_key))
        .child(span().text("|"))
        .child(h2().attr("data-i18n", title2_key));

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
    quench_srv::actix::routers::ui::serve_assets(path, "dist/assets").await
}

pub(super) fn render_page(
    mut builder: actix_web::HttpResponseBuilder,
    content: Element,
    page_kind: UiPageKind,
) -> HttpResponse {
    let shell = match page_kind {
        UiPageKind::Home => &*UI_SHELL_HOME,
        UiPageKind::ModelsDashboard => &*UI_SHELL_MODELS_DASHBOARD,
        UiPageKind::VllmManagement => &*UI_SHELL_VLLM_MANAGEMENT,
        UiPageKind::Auth => &*UI_SHELL_AUTH,
    };
    builder
        .content_type(ContentType::html())
        .body(shell.page(div().class("page").child(content)))
}

pub(super) enum UiPageKind {
    Home,
    ModelsDashboard,
    VllmManagement,
    Auth,
}

pub(super) fn ui_login_redirect() -> HttpResponse {
    quench_srv::actix::routers::ui::ui_login_redirect()
}
