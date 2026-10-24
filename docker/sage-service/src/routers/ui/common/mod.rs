use forge_ui::{NavItem, NavSection, PageWidth, ShellConfig, Sidebar, UiShell};
pub use quench_auth::domain::jwt::Claims;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_core::async_trait;
use quench_http::prelude::{
    FromRequest, HttpError, Path, Request, Response, get, http::StatusCode,
};
pub use quench_starter::http::routers::ui::{
    is_ui_authenticated, ui_asset_path, ui_login_redirect, ui_login_redirect_for, ui_path,
};
use quench_web::prelude::*;
use std::sync::LazyLock;

pub mod css;
pub mod format;

pub use forge_ui::{SUPPORTED_LOCALES, supported_locales};

static UI_SHELL: LazyLock<UiShell> = LazyLock::new(|| {
    css::ensure_sage_css();

    UiShell::new(
        ShellConfig::new("Sage", "sage.css", "wand-magic-sparkles")
            .nav(vec![NavSection::new(vec![NavItem::new(
                "comments",
                "ui_nav_chat",
                ui_path("/home"),
            )])])
            .dashboard(forge_ui::gatehouse_dashboard_url())
            .profile(forge_ui::gatehouse_profile_url()),
    )
});

#[get("/ui/assets/{path:.*}")]
pub async fn assets(Path(path): Path<String>) -> Response {
    quench_starter::http::routers::ui::serve_assets(&path, "dist/assets").await
}

pub fn render_page(status: quench_http::http::StatusCode, content: Element) -> Response {
    Response::html(status, UI_SHELL.page(content))
}

/// Edge to edge, scrolling its own panes: the chat screen.
/// `nav` takes the sidebar's place: the chat's own project and history list.
pub fn render_page_fill(
    status: quench_http::http::StatusCode,
    content: Element,
    nav: Element,
) -> Response {
    let sidebar = Sidebar::default().replacing_nav(nav);
    Response::html(status, UI_SHELL.page_in(content, PageWidth::Fill, &sidebar))
}

/// The signed-in identity from the realm session cookie. `None` means "not
/// signed in" - auth-disabled is already folded into a synthetic `Claims`.
pub async fn actor(request: &Request, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
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

/// Claims or the 401 - travels as the success value since
/// `HttpError::into_response` only renders fixed text.
#[derive(Debug)]
pub enum RequiredClaims {
    Claims(Claims),
    Unauthorized,
}

impl RequiredClaims {
    pub fn or_401(self) -> Result<Claims, Response> {
        match self {
            Self::Claims(claims) => Ok(claims),
            Self::Unauthorized => Err(Response::new(StatusCode::UNAUTHORIZED)),
        }
    }
}

#[async_trait]
impl FromRequest for RequiredClaims {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self::Unauthorized);
        };
        Ok(actor(req, &config)
            .await
            .map_or(Self::Unauthorized, Self::Claims))
    }
}

/// Claims or an hx-aware redirect - for pages outside `wrap_auth`'s scopes
/// that check auth themselves; travels as the success value, same reason.
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
        actor(req, &config).await.map_or_else(
            || Ok(Self::Redirect(ui_login_redirect_for(req))),
            |claims| Ok(Self::Claims(claims)),
        )
    }
}

pub fn register_routes() {
    let _ = assets as fn(_) -> _;
}

impl std::fmt::Debug for ActorOrRedirect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ActorOrRedirect { .. }")
    }
}
