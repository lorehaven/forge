use async_trait::async_trait;
pub use quench_auth::domain::jwt::Claims;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::routers::ui::get_user_from_req;
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

pub const SUPPORTED_LOCALES: [&str; 5] = ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"];

pub fn supported_locales() -> Vec<String> {
    SUPPORTED_LOCALES.iter().map(|s| s.to_string()).collect()
}

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_sage_css();

    AppShellBuilder::new()
        .title("Sage")
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(Some("ui_header_home"), true, false, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/sage.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
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

    header()
        .child(div().class("left-panel").child(title))
        .child(
            div()
                .class("right-panel")
                .child_opt(
                    show_locale_switch.then(|| locale_switch(Some(supported_locales()), None)),
                )
                .child_opt(show_home.then(|| {
                    a().attr("href", ui_path("/home"))
                        .class("button")
                        .attr("data-i18n", "ui_home_button")
                }))
                .child_opt(show_logout.then(|| {
                    a().attr("href", ui_path("/logout"))
                        .class("button")
                        .attr("data-i18n", "ui_logout")
                })),
        )
}

#[get("/ui/assets/{path:.*}")]
pub async fn assets(Path(path): Path<String>) -> Response {
    quench_starter::http::routers::ui::serve_assets(&path, "dist/assets").await
}

pub fn render_page(status: http::StatusCode, content: Element) -> Response {
    Response::html(
        status,
        UI_SHELL_HOME.page(div().class("page").child(content)),
    )
}

/// The signed-in identity from the realm session cookie. `None` means "not
/// signed in" - auth-disabled is already folded into a synthetic `Claims`.
pub async fn actor(request: &Request, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
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

/// Claims or the 401 - travels as the success value since
/// `HttpError::into_response` only renders fixed text.
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
        match actor(req, &config).await {
            Some(claims) => Ok(Self::Claims(claims)),
            None => Ok(Self::Unauthorized),
        }
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
        match actor(req, &config).await {
            Some(claims) => Ok(Self::Claims(claims)),
            None => Ok(Self::Redirect(ui_login_redirect_for(req))),
        }
    }
}

pub fn register_routes() {
    let _ = assets as fn(_) -> _;
}
