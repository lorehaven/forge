//! Page shell shared with the rest of the estate - same builder, theme, header, stylesheet layout.

use async_trait::async_trait;
use forge_ui::{NavItem, NavSection, ShellConfig, UiShell};
pub use quench_auth::domain::jwt::Claims;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_http::prelude::{FromRequest, HttpError, Path, Request, Response, get};
pub use quench_starter::http::routers::ui::{
    is_ui_authenticated, ui_asset_path, ui_login_redirect, ui_login_redirect_for, ui_path,
};
use quench_web::prelude::*;
use std::sync::LazyLock;

pub mod format;

pub use forge_ui::{SUPPORTED_LOCALES, supported_locales};

/// Writes the service stylesheet (forge-ui's plus `conveyor.css`) to `dist/assets/css`.
pub fn conveyor_stylesheet() -> String {
    forge_ui::stylesheet_with(include_str!("conveyor.css"), &[])
}

pub fn ensure_conveyor_css() {
    forge_ui::write_stylesheet("conveyor.css", &conveyor_stylesheet());
}

static UI_SHELL: LazyLock<UiShell> = LazyLock::new(|| {
    ensure_conveyor_css();

    UiShell::new(
        ShellConfig::new("Conveyor", "conveyor.css", "diagram-project")
            .nav(vec![NavSection::new(vec![
                NavItem::new("diagram-project", "ui_nav_pipelines", ui_path("/home")),
                NavItem::new("key", "ui_nav_credentials", ui_path("/credentials")),
            ])])
            .dashboard(forge_ui::gatehouse_dashboard_url())
            .profile(forge_ui::gatehouse_profile_url()),
    )
});

/// Writes `dist/assets` before the first request, or an early stylesheet request answers stale.
pub fn ensure_assets() {
    LazyLock::force(&UI_SHELL);
}

#[get("/ui/assets/{path:.*}")]
pub async fn assets(Path(path): Path<String>) -> Response {
    quench_starter::http::routers::ui::serve_assets(&path, "dist/assets").await
}

/// `.status` carries the shape, `.status-<state>` the color; the label is a translation key.
pub fn status_pill(status: crate::domain::Status) -> Element {
    span()
        .class(format!("status status-{status}"))
        .attr("data-i18n", format!("ui_status_{status}"))
}

pub fn render_page(status: http::StatusCode, content: Element) -> Response {
    Response::html(status, UI_SHELL.page(content))
}

/// Bearer token or realm cookie. `None` means "not signed in", not "auth
/// disabled" - `get_user_from_req` folds that into a synthetic all-access `Claims`.
pub async fn actor(request: &Request, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
}

/// For a page that only needs to gate rendering, not the identity behind it.
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

/// Not a plain `Result<_, HttpError>` - `into_response` always renders fixed
/// text, never a redirect, so the redirect has to travel as the success value.
pub enum ActorOrRedirect {
    Claims(Claims),
    Redirect(Response),
}

impl ActorOrRedirect {
    pub fn or_redirect(self) -> Result<Claims, Response> {
        match self {
            Self::Claims(claims) => Ok(claims),
            Self::Redirect(resp) => Err(resp),
        }
    }
}

#[async_trait]
impl FromRequest for ActorOrRedirect {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self::Redirect(ui_login_redirect_for(req)));
        };
        match actor(req, &config).await {
            Some(claims) => Ok(Self::Claims(claims)),
            None => Ok(Self::Redirect(ui_login_redirect_for(req))),
        }
    }
}

/// `PageAuth`'s hx-aware sibling - redirects an expired fragment poll with
/// `HX-Redirect` instead of a 302 it would try to parse as the fragment body.
pub enum PageGate {
    Authenticated,
    Redirect(Response),
}

impl PageGate {
    pub fn or_redirect(self) -> Result<(), Response> {
        match self {
            Self::Authenticated => Ok(()),
            Self::Redirect(resp) => Err(resp),
        }
    }
}

#[async_trait]
impl FromRequest for PageGate {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self::Redirect(ui_login_redirect_for(req)));
        };
        if is_ui_authenticated(req, &config).await {
            Ok(Self::Authenticated)
        } else {
            Ok(Self::Redirect(ui_login_redirect_for(req)))
        }
    }
}

pub fn register_routes() {
    let _ = assets as fn(_) -> _;
}
