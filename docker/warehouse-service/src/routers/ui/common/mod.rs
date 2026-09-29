use async_trait::async_trait;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{
    FromRequest, HttpError, Path, Request, Response, get, http::StatusCode,
};
pub use quench_starter::http::routers::ui::{is_ui_authenticated, ui_asset_path, ui_path};
use quench_web::prelude::*;
use std::sync::LazyLock;

pub mod css;

pub use forge_ui::{SUPPORTED_LOCALES, supported_locales};

static UI_SHELL_DOCKER: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_warehouse_css();

    forge_ui::app_shell(
        "Warehouse",
        "warehouse.css",
        Some(ui_header(Some("ui_header_docker"), true, true, true)),
    )
});

static UI_SHELL_CRATES: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_warehouse_css();

    forge_ui::app_shell(
        "Warehouse — Crates",
        "warehouse.css",
        Some(ui_header(Some("ui_header_crates"), true, true, true)),
    )
});

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_warehouse_css();

    forge_ui::app_shell(
        "Warehouse",
        "warehouse.css",
        Some(ui_header(Some("ui_header_home"), true, true, true)),
    )
});

static UI_SHELL_FILES: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_warehouse_css();

    forge_ui::app_shell(
        "Warehouse — Files",
        "warehouse.css",
        Some(ui_header(Some("ui_header_files"), true, true, true)),
    )
});

static UI_SHELL_ARTIFACTS: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_warehouse_css();

    forge_ui::app_shell(
        "Warehouse — Artifacts",
        "warehouse.css",
        Some(ui_header(Some("ui_header_artifacts"), true, true, true)),
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

#[get("/ui/assets/{path:.*}")]
pub async fn assets(Path(path): Path<String>) -> Response {
    quench_starter::http::routers::ui::serve_assets(&path, "dist/assets").await
}

pub fn render_page(status: StatusCode, content: Element, page_kind: UiPageKind) -> Response {
    let shell = match page_kind {
        UiPageKind::Home => &*UI_SHELL_HOME,
        UiPageKind::Docker => &*UI_SHELL_DOCKER,
        UiPageKind::Crates => &*UI_SHELL_CRATES,
        UiPageKind::Files => &*UI_SHELL_FILES,
        UiPageKind::Artifacts => &*UI_SHELL_ARTIFACTS,
    };
    Response::html(status, shell.page(div().class("page").child(content)))
}

pub enum UiPageKind {
    Home,
    Docker,
    Crates,
    Files,
    Artifacts,
}

pub fn ui_login_redirect() -> Response {
    quench_starter::http::routers::ui::ui_login_redirect()
}

/// Whether the request carries a usable realm session - for a page that only
/// needs to gate rendering, not the identity behind it.
pub struct PageAuth(pub bool);

#[async_trait]
impl FromRequest for PageAuth {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self(false));
        };
        Ok(Self(is_ui_authenticated(req, &config).await))
    }
}

pub fn register_routes() {
    let _ = assets as fn(_) -> _;
}
