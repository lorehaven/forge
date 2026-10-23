//! Page shell, shared with the rest of the estate: same builder, same theme,
//! same header, same generated stylesheet layout.

use forge_ui::{NavItem, NavSection, ShellConfig, UiShell};
use quench_http::prelude::{Path, Response, get};
pub use quench_starter::http::routers::ui::{is_ui_authenticated, ui_asset_path, ui_path};
use quench_web::prelude::*;
use std::sync::LazyLock;

pub use forge_ui::{SUPPORTED_LOCALES, supported_locales};

/// Writes the service stylesheet (forge-ui's plus `gatehouse.css`) to `dist/assets/css`.
pub fn ensure_gatehouse_css() {
    forge_ui::write_css_with("gatehouse.css", include_str!("gatehouse.css"));
}

fn config() -> ShellConfig {
    ShellConfig::new("Gatehouse", "gatehouse.css", "shield-halved")
}

// Gatehouse *is* the services dashboard, so there is no dashboard icon in its own top bar.
static UI_SHELL: LazyLock<UiShell> = LazyLock::new(|| {
    ensure_gatehouse_css();

    UiShell::new(
        config()
            .nav(vec![NavSection::new(vec![
                NavItem::new("table-cells-large", "ui_home_title", ui_path("/home")),
                NavItem::new("users", "ui_admin_title", ui_path("/admin/users")),
            ])])
            .profile(ui_path("/account")),
    )
});

// Login, registration, password reset: no session, so no sidebar.
static UI_SHELL_AUTH: LazyLock<UiShell> = LazyLock::new(|| {
    ensure_gatehouse_css();

    UiShell::auth(config())
});

/// Writes `dist/assets` before the first request, or an early stylesheet fetch gets stale content.
pub fn ensure_assets() {
    LazyLock::force(&UI_SHELL);
    LazyLock::force(&UI_SHELL_AUTH);
}

#[get("/ui/assets/{path:.*}")]
pub async fn assets(Path(path): Path<String>) -> Response {
    quench_starter::http::routers::ui::serve_assets(&path, "dist/assets").await
}

pub fn render_page(
    status: quench_http::http::StatusCode,
    content: Element,
    page_kind: UiPageKind,
) -> Response {
    let html = match page_kind {
        UiPageKind::Auth => UI_SHELL_AUTH.page(content),
        UiPageKind::Home | UiPageKind::Admin | UiPageKind::Account => UI_SHELL.page(content),
    };
    Response::html(status, html)
}

/// The fields of an urlencoded form post, by name.
pub type FormFields = std::collections::HashMap<String, String>;

#[derive(Clone, Copy, Debug)]
pub enum UiPageKind {
    Home,
    Auth,
    Admin,
    Account,
}

pub fn register_routes() {
    let _ = assets as fn(_) -> _;
}

/// This listener's own scheme, for building absolute URLs; `x-forwarded-proto` still wins if set.
#[derive(Debug)]
pub struct ExternalScheme(pub &'static str);
