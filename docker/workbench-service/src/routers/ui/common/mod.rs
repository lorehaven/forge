use async_trait::async_trait;
pub use quench_auth::domain::jwt::Claims;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_http::prelude::{FromRequest, HttpError, Path, Request, Response, get};
pub use quench_starter::http::routers::ui::{
    is_ui_authenticated, ui_asset_path, ui_login_redirect_for, ui_path,
};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::LazyLock;

mod css;

use forge_ui::{NavItem, NavSection, ShellConfig, Tone, UiShell, alert, txt};
pub use forge_ui::{SUPPORTED_LOCALES, supported_locales};

static UI_SHELL: LazyLock<UiShell> = LazyLock::new(|| {
    css::ensure_workbench_css();

    UiShell::new(
        ShellConfig::new("Workbench", "workbench.css", "list-check")
            .nav(vec![NavSection::new(vec![NavItem::new(
                "table-columns",
                "ui_nav_projects",
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

pub fn render_page(status: http::StatusCode, content: Element) -> Response {
    Response::html(status, UI_SHELL.page(content))
}

/// Full-width variant for pages that scroll their own panes (the board).
pub fn render_page_wide(status: http::StatusCode, content: Element) -> Response {
    Response::html(status, UI_SHELL.page_wide(content))
}

#[must_use]
pub fn ui_login_redirect() -> Response {
    quench_starter::http::routers::ui::ui_login_redirect()
}

/// The signed-in identity from the session cookie. `None` means "not signed
/// in", not "auth disabled" - that folds into a synthetic all-access `Claims`.
pub async fn actor(request: &Request, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
}

/// Whether the request carries a usable realm session - for pages that only
/// gate rendering, not needing the identity itself.
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

/// Claims, or the redirect to send instead. Not a plain `Result<_, HttpError>`
/// since `HttpError::into_response` can't render a redirect.
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

/// A post-mutation redirect's status code, rendered as an untranslated
/// banner - not run through i18n since it's operational, not chrome.
#[derive(Debug, Deserialize, Default)]
pub struct Notice {
    #[serde(default)]
    pub ok: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[must_use]
pub fn notice_banner(notice: &Notice) -> Option<Element> {
    if let Some(code) = &notice.error {
        return Some(alert(Tone::Error, txt(format!("error: {code}"))));
    }
    notice
        .ok
        .as_ref()
        .map(|code| alert(Tone::Success, txt(code.clone())))
}

/// Assignee `<select>` + "assign to me" shortcut. "Unassigned"/"Me" come
/// first; `current_user` is excluded from the alphabetical tail.
pub fn assignee_field(
    current_user: &str,
    users: &[crate::domain::realm_users::RealmUser],
    selected: Option<&str>,
) -> Element {
    let mut field = select()
        .attr("id", "wb-assignee")
        .attr("name", "assignee")
        .class("fui-select");

    field = field.child({
        let mut opt = option()
            .attr("value", "")
            .attr("data-i18n", "ui_field_unassigned");
        if selected.is_none_or(str::is_empty) {
            opt = opt.attr("selected", "true");
        }
        opt
    });

    field = field.child({
        let mut opt = option().attr("value", current_user).text("Me");
        if selected == Some(current_user) {
            opt = opt.attr("selected", "true");
        }
        opt
    });

    for user in users {
        if user.username == current_user {
            continue;
        }
        let mut opt = option()
            .attr("value", &user.username)
            .text(user.label().to_string());
        if selected == Some(user.username.as_str()) {
            opt = opt.attr("selected", "true");
        }
        field = field.child(opt);
    }

    div().class("wb-field-control").child(field).child(
        button()
            .attr("type", "button")
            .attr(
                "onclick",
                format!(
                    "document.getElementById('wb-assignee').value='{}'",
                    current_user.replace('\'', "\\'")
                ),
            )
            .class("fui-btn fui-btn-secondary wb-assign-me")
            .attr("data-i18n", "ui_assign_to_me"),
    )
}

pub fn register_routes() {
    let _ = assets as fn(_) -> _;
}

impl std::fmt::Debug for ActorOrRedirect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ActorOrRedirect { .. }")
    }
}
