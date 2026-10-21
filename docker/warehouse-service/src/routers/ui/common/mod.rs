use async_trait::async_trait;
use forge_ui::{NavItem, NavSection, ShellConfig, UiShell};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{
    FromRequest, HttpError, Path, Request, Response, get, http::StatusCode,
};
pub use quench_starter::http::routers::ui::{is_ui_authenticated, ui_asset_path, ui_path};
use quench_web::prelude::*;
use std::sync::LazyLock;

pub use forge_ui::{SUPPORTED_LOCALES, supported_locales};

/// Guarded by `Once` so concurrent callers can't interleave writes to the same file.
pub fn ensure_warehouse_css() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        forge_ui::write_css_with("warehouse.css", include_str!("warehouse.css"));
    });
}

static UI_SHELL: LazyLock<UiShell> = LazyLock::new(|| {
    ensure_warehouse_css();

    let mut items = Vec::new();
    for (enabled, icon, key, path) in [
        (
            crate::routers::docker_enabled(),
            "docker",
            "ui_header_docker",
            "/docker/catalog",
        ),
        (
            crate::routers::crates_enabled(),
            "box-open",
            "ui_header_crates",
            "/crates/catalog",
        ),
        (
            crate::routers::files_enabled(),
            "folder-open",
            "ui_header_files",
            "/files/storages",
        ),
        (
            crate::routers::artifacts_enabled(),
            "cube",
            "ui_header_artifacts",
            "/artifacts/catalog",
        ),
        (
            crate::routers::rivets_enabled(),
            "screwdriver-wrench",
            "ui_header_rivets",
            "/rivets/catalog",
        ),
    ] {
        if enabled {
            items.push(NavItem::new(icon, key, ui_path(path)));
        }
    }

    UiShell::new(
        ShellConfig::new("Warehouse", "warehouse.css", "warehouse")
            .nav(vec![NavSection::new(items)])
            .dashboard(forge_ui::gatehouse_dashboard_url())
            .profile(forge_ui::gatehouse_profile_url()),
    )
});

#[get("/ui/assets/{path:.*}")]
pub async fn assets(Path(path): Path<String>) -> Response {
    quench_starter::http::routers::ui::serve_assets(&path, "dist/assets").await
}

pub fn render_page(status: StatusCode, content: Element) -> Response {
    Response::html(status, UI_SHELL.page_wide(content))
}

#[must_use]
pub fn ui_login_redirect() -> Response {
    quench_starter::http::routers::ui::ui_login_redirect()
}

/// Whether the request carries a usable realm session - for a page that only
/// needs to gate rendering, not the identity behind it.
#[derive(Debug)]
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
