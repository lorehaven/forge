//! Page shell shared with the rest of the estate - same builder, theme, header, stylesheet layout.

use async_trait::async_trait;
pub use quench_auth::domain::jwt::Claims;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_http::prelude::{FromRequest, HttpError, Path, Request, Response, get};
pub use quench_starter::http::routers::ui::{
    is_ui_authenticated, ui_asset_path, ui_login_redirect, ui_login_redirect_for, ui_path,
};
use quench_web::nav_button;
use quench_web::prelude::*;
use std::sync::LazyLock;

pub mod css;
pub mod format;
pub mod nav;

const SUPPORTED_LOCALES: [&str; 5] = ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"];

fn supported_locales() -> Vec<String> {
    SUPPORTED_LOCALES.iter().map(|s| s.to_string()).collect()
}

fn shell(title_key: &str, show_home: bool) -> AppShell {
    css::ensure_conveyor_css();

    AppShellBuilder::new()
        .title("Conveyor")
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(title_key, show_home))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/conveyor.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
}

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| shell("ui_header_home", false));

fn ui_header(title_key: &str, show_home: bool) -> Element {
    header()
        .child(
            div()
                .class("left-panel")
                .child(nav_button())
                .child(h2().attr("data-i18n", title_key)),
        )
        .child(
            div()
                .class("right-panel")
                .child_opt(show_home.then(|| {
                    a().attr("href", ui_path("/home"))
                        .class("button")
                        .attr("data-i18n", "ui_home_button")
                }))
                .child(
                    a().attr("href", ui_path("/logout"))
                        .class("button")
                        .attr("data-i18n", "ui_logout"),
                ),
        )
        .child(nav::panel())
}

/// Writes `dist/assets` before the first request, or an early stylesheet request answers stale.
pub fn ensure_assets() {
    LazyLock::force(&UI_SHELL_HOME);
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
    Response::html(
        status,
        UI_SHELL_HOME.page(div().class("page").child(content)),
    )
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
