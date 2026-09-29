//! Page shell, shared with the rest of the estate: same builder, same theme,
//! same header, same generated stylesheet layout.

use quench_http::prelude::{Path, Response, get};
pub use quench_starter::http::routers::ui::{is_ui_authenticated, ui_asset_path, ui_path};
use quench_web::prelude::*;
use std::sync::LazyLock;

pub mod css;

pub use forge_ui::{SUPPORTED_LOCALES, supported_locales};

fn shell(header: Option<Element>) -> AppShell {
    css::ensure_gatehouse_css();

    forge_ui::app_shell("Gatehouse", "gatehouse.css", header)
}

static UI_SHELL_HOME: LazyLock<AppShell> =
    LazyLock::new(|| shell(Some(ui_header("ui_home_title", true, false, true))));

// The admin pages sit under the home page, so the header offers a way back to it
// as well as a way out of the realm.
static UI_SHELL_ADMIN: LazyLock<AppShell> =
    LazyLock::new(|| shell(Some(ui_header("ui_admin_title", true, true, true))));

// The account page sits under the home page too, same as admin.
static UI_SHELL_ACCOUNT: LazyLock<AppShell> =
    LazyLock::new(|| shell(Some(ui_header("ui_account_title", true, true, true))));

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

    forge_ui::TopBar {
        show_home,
        show_locale_switch,
        user_menu_profile: show_logout.then(|| ui_path("/account")),
        ..Default::default()
    }
    .build(title)
}

/// Writes `dist/assets` before the first request, or an early stylesheet fetch gets stale content.
pub fn ensure_assets() {
    LazyLock::force(&UI_SHELL_HOME);
    LazyLock::force(&UI_SHELL_AUTH);
    LazyLock::force(&UI_SHELL_ADMIN);
    LazyLock::force(&UI_SHELL_ACCOUNT);
}

#[get("/ui/assets/{path:.*}")]
pub async fn assets(Path(path): Path<String>) -> Response {
    quench_starter::http::routers::ui::serve_assets(&path, "dist/assets").await
}

pub fn render_page(status: http::StatusCode, content: Element, page_kind: UiPageKind) -> Response {
    let shell = match page_kind {
        UiPageKind::Home => &*UI_SHELL_HOME,
        UiPageKind::Auth => &*UI_SHELL_AUTH,
        UiPageKind::Admin => &*UI_SHELL_ADMIN,
        UiPageKind::Account => &*UI_SHELL_ACCOUNT,
    };
    Response::html(status, shell.page(div().class("page").child(content)))
}

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
pub struct ExternalScheme(pub &'static str);
