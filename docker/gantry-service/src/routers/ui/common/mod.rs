use forge_ui::{NavItem, NavSection, ShellConfig, UiShell};
pub use quench_auth::domain::jwt::Claims;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_core::async_trait;
use quench_http::prelude::{
    FromRequest, HttpError, Path, Request, Response, get, http::StatusCode,
};
pub use quench_starter::http::routers::ui::{
    is_ui_authenticated, ui_asset_path, ui_login_redirect_for, ui_path,
};
use quench_web::prelude::*;
use std::fmt::Write as _;
use std::sync::LazyLock;

pub use forge_ui::{SUPPORTED_LOCALES, supported_locales};

/// Writes the service stylesheet (forge-ui's plus `gantry.css`) to `dist/assets/css`.
pub fn ensure_gantry_css() {
    forge_ui::write_css_with("gantry.css", include_str!("gantry.css"));
}

static UI_SHELL: LazyLock<UiShell> = LazyLock::new(|| {
    ensure_gantry_css();

    UiShell::new(
        ShellConfig::new("Gantry", "gantry.css", "anchor")
            .nav(vec![NavSection::new(vec![
                NavItem::new("cubes", "ui_nav_targets", ui_path("/home")),
                NavItem::new("list-check", "ui_nav_operations", ui_path("/operations")),
            ])])
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

#[must_use]
pub fn ui_login_redirect() -> Response {
    quench_starter::http::routers::ui::ui_login_redirect()
}

/// The signed-in identity from the session cookie. `None` means "not signed in", not "auth disabled" -
/// that folds into a synthetic all-access `Claims`.
pub async fn actor(request: &Request, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
}

/// Whether the request carries a usable realm session - for pages that only gate rendering, not needing
/// the identity itself.
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

/// Claims, or the redirect to send instead. Not a plain `Result<_, HttpError>` since
/// `HttpError::into_response` can't render a redirect.
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

/// A post-mutation redirect's outcome, shown as an untranslated banner: operational, not chrome.
#[derive(Debug, quench_serde::Deserialize, Default)]
pub struct Notice {
    #[serde(default)]
    pub ok: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[must_use]
pub fn notice_banner(notice: &Notice) -> Option<Element> {
    if let Some(message) = &notice.error {
        return Some(p().class("gt-notice gt-notice-error").text(message.clone()));
    }
    notice
        .ok
        .as_ref()
        .map(|message| p().class("gt-notice gt-notice-ok").text(message.clone()))
}

/// Redirects to `path` (relative to the UI), with an optional banner.
#[must_use]
pub fn redirect(path: &str, notice: Option<(&str, &str)>) -> Response {
    let mut location = ui_path(path);
    if let Some((kind, message)) = notice {
        let _ = write!(location, "?{kind}={}", percent_encode(message));
    }
    Response::new(StatusCode::FOUND).header("Location", location)
}

#[must_use]
pub fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// A state is a coloured dot and a translated word.
#[must_use]
pub fn badge(kind: &str, value: &str) -> Element {
    span()
        .class(format!("gt-state gt-state-{value}"))
        .child(span().class("gt-dot"))
        .child(span().attr("data-i18n", format!("ui_{kind}_{value}")))
}

/// A button that posts to `path`. `style` is `""`, `primary` or `danger`.
#[must_use]
pub fn action(path: &str, label_key: &str, style: &str) -> Element {
    action_with(path, label_key, style, &[], None)
}

/// A button that posts `fields` to `path`, asking first (in the browser) if `confirm` is given. The question
/// names the thing, so it is not a reflex "are you sure".
#[must_use]
pub fn action_with(
    path: &str,
    label_key: &str,
    style: &str,
    fields: &[(&str, &str)],
    confirm: Option<&str>,
) -> Element {
    let class = if style.is_empty() {
        "gt-btn".to_string()
    } else {
        format!("gt-btn gt-btn-{style}")
    };
    let mut submit = button()
        .attr("type", "submit")
        .class(class)
        .attr("data-i18n", label_key.to_string());
    if let Some(question) = confirm {
        submit = submit
            .attr("data-confirm", question)
            .attr("onclick", "return confirm(this.dataset.confirm)");
    }
    let mut form = form()
        .attr("method", "post")
        .attr("action", ui_path(path))
        .class("gt-inline");
    for (name, value) in fields {
        form = form.child(
            input()
                .attr("type", "hidden")
                .attr("name", *name)
                .attr("value", *value),
        );
    }
    form.child(submit)
}

#[must_use]
pub fn table(headers: &[&str]) -> Element {
    let mut row = element("tr");
    for header in headers {
        row = row.child(element("th").attr("data-i18n", *header));
    }
    element("table")
        .class("gt-table")
        .child(element("thead").child(row))
}

#[must_use]
pub fn row(cells: Vec<Element>) -> Element {
    let mut row = element("tr");
    for cell in cells {
        row = row.child(element("td").child(cell));
    }
    row
}

pub fn cell_text(text: impl ToString) -> Element {
    span().text(text)
}

pub fn register_routes() {
    let _ = assets as fn(_) -> _;
}

impl std::fmt::Debug for ActorOrRedirect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ActorOrRedirect { .. }")
    }
}
