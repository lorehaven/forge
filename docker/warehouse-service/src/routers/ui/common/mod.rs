use actix_web::{HttpResponse, Responder, get, http::header::ContentType, web};
pub use quench_srv::actix::routers::ui::{is_ui_authenticated, ui_asset_path, ui_path};
use quench_web::prelude::*;
use std::sync::LazyLock;

mod crates_js;
mod docker_js;
mod files_js;
mod warehouse_css;

static UI_SHELL_DOCKER: LazyLock<AppShell> = LazyLock::new(|| {
    warehouse_css::ensure_warehouse_css();
    docker_js::ensure_docker_js();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_docker"), true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .scripts(vec![Script::new(&ui_asset_path("/js/docker.js"))])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_CRATES: LazyLock<AppShell> = LazyLock::new(|| {
    warehouse_css::ensure_warehouse_css();
    crates_js::ensure_crates_js();

    AppShellBuilder::new()
        .title("Warehouse — Crates")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_crates"), true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .scripts(vec![Script::new(&ui_asset_path("/js/crates.js"))])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_FILES: LazyLock<AppShell> = LazyLock::new(|| {
    warehouse_css::ensure_warehouse_css();
    files_js::ensure_files_js();

    AppShellBuilder::new()
        .title("Warehouse — Files")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_files"), true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .scripts(vec![Script::new(&ui_asset_path("/js/files.js"))])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| {
    warehouse_css::ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_home"), true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_AUTH: LazyLock<AppShell> = LazyLock::new(|| {
    warehouse_css::ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(None, false, false))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
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
                    a().attr("href", &ui_path("/home"))
                        .class("button")
                        .attr("data-i18n", "ui_home_button")
                }))
                .child_opt(show_logout.then(|| {
                    a().attr("href", &ui_path("/logout"))
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
        UiPageKind::Docker => &*UI_SHELL_DOCKER,
        UiPageKind::Crates => &*UI_SHELL_CRATES,
        UiPageKind::Files => &*UI_SHELL_FILES,
        UiPageKind::Auth => &*UI_SHELL_AUTH,
    };
    builder
        .content_type(ContentType::html())
        .body(shell.page(div().class("page").child(content)))
}

pub(super) enum UiPageKind {
    Home,
    Docker,
    Crates,
    Files,
    Auth,
}

pub(super) fn ui_login_redirect() -> HttpResponse {
    quench_srv::actix::routers::ui::ui_login_redirect()
}
