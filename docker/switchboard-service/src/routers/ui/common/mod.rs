use forge_ui::{NavItem, NavSection, ShellConfig, UiShell};
use quench_http::prelude::{Path, Response, get};
pub use quench_starter::http::routers::ui::{is_ui_authenticated, ui_asset_path, ui_path};
use quench_web::prelude::*;
use std::sync::LazyLock;

pub use forge_ui::{SUPPORTED_LOCALES, supported_locales};

/// Writes the service stylesheet (forge-ui's plus `switchboard.css`) to `dist/assets/css`.
#[must_use]
pub fn switchboard_stylesheet() -> String {
    forge_ui::stylesheet_with(include_str!("switchboard.css"), &[])
}

pub fn ensure_switchboard_css() {
    forge_ui::write_stylesheet("switchboard.css", &switchboard_stylesheet());
}

static UI_SHELL: LazyLock<UiShell> = LazyLock::new(|| {
    ensure_switchboard_css();

    let mut items = Vec::new();
    if crate::routers::models_dashboard_enabled() {
        items.push(NavItem::new(
            "cubes",
            "ui_header_models",
            ui_path("/models/dashboard"),
        ));
    }
    if crate::routers::vllm_management_enabled() {
        items.push(NavItem::new(
            "microchip",
            "ui_header_vllm",
            ui_path("/vllm/manage"),
        ));
    }

    UiShell::new(
        ShellConfig::new("Switchboard", "switchboard.css", "shuffle")
            .nav(vec![NavSection::new(items)])
            .dashboard(forge_ui::gatehouse_dashboard_url())
            .profile(forge_ui::gatehouse_profile_url()),
    )
});

#[get("/ui/assets/{path:.*}")]
pub async fn assets(Path(path): Path<String>) -> Response {
    quench_starter::http::routers::ui::serve_assets(&path, "dist/assets").await
}

pub(super) fn render_page(status: http::StatusCode, content: Element) -> Response {
    Response::html(status, UI_SHELL.page_wide(content))
}

pub(super) fn ui_login_redirect() -> Response {
    quench_starter::http::routers::ui::ui_login_redirect()
}
