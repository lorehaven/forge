use quench_http::prelude::{Path, Response, get};
pub use quench_starter::http::routers::ui::{is_ui_authenticated, ui_asset_path, ui_path};
use quench_web::prelude::*;
use std::sync::LazyLock;

pub mod css;

pub use forge_ui::{SUPPORTED_LOCALES, supported_locales};

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_switchboard_css();

    forge_ui::app_shell(
        "Switchboard",
        "switchboard.css",
        Some(ui_header(Some("ui_header_home"), true, true, true)),
    )
});

static UI_SHELL_MODELS_DASHBOARD: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_switchboard_css();

    forge_ui::app_shell(
        "Switchboard",
        "switchboard.css",
        Some(ui_header_split(
            "ui_header_dashboard",
            "ui_header_models",
            true,
            true,
            true,
        )),
    )
});

static UI_SHELL_VLLM_MANAGEMENT: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_switchboard_css();

    forge_ui::app_shell(
        "Switchboard",
        "switchboard.css",
        Some(ui_header_split(
            "ui_header_dashboard",
            "ui_header_vllm",
            true,
            true,
            true,
        )),
    )
});

pub fn ui_header(
    title_key: Option<&str>,
    show_locale_switch: bool,
    show_home: bool,
    show_logout: bool,
) -> Element {
    let title = match title_key {
        Some(key) => h2().attr("data-i18n", key),
        None => h2().attr("data-i18n", "header_label"),
    };

    forge_ui::TopBar {
        show_home,
        show_locale_switch,
        user_menu_profile: show_logout.then(forge_ui::gatehouse_profile_url),
        ..Default::default()
    }
    .build(title)
}

pub fn ui_header_split(
    title1_key: &str,
    title2_key: &str,
    show_locale_switch: bool,
    show_home: bool,
    show_logout: bool,
) -> Element {
    let title = div()
        .class("header-split")
        .child(h2().attr("data-i18n", title1_key))
        .child(span().text("|"))
        .child(h2().attr("data-i18n", title2_key));

    forge_ui::TopBar {
        show_home,
        show_locale_switch,
        user_menu_profile: show_logout.then(forge_ui::gatehouse_profile_url),
        ..Default::default()
    }
    .build(title)
}

#[get("/ui/assets/{path:.*}")]
pub async fn assets(Path(path): Path<String>) -> Response {
    quench_starter::http::routers::ui::serve_assets(&path, "dist/assets").await
}

pub(super) fn render_page(
    status: http::StatusCode,
    content: Element,
    page_kind: UiPageKind,
) -> Response {
    let shell = match page_kind {
        UiPageKind::Home => &*UI_SHELL_HOME,
        UiPageKind::ModelsDashboard => &*UI_SHELL_MODELS_DASHBOARD,
        UiPageKind::VllmManagement => &*UI_SHELL_VLLM_MANAGEMENT,
    };
    Response::html(status, shell.page(div().class("page").child(content)))
}

pub(super) enum UiPageKind {
    Home,
    ModelsDashboard,
    VllmManagement,
}

pub(super) fn ui_login_redirect() -> Response {
    quench_starter::http::routers::ui::ui_login_redirect()
}
