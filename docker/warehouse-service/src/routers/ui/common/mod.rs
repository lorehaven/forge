use async_trait::async_trait;
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{
    FromRequest, HttpError, Path, Request, Response, get, http::StatusCode,
};
pub use quench_starter::http::routers::ui::{is_ui_authenticated, ui_asset_path, ui_path};
use quench_web::prelude::*;
use std::sync::LazyLock;

pub mod css;

pub const SUPPORTED_LOCALES: [&str; 5] = ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"];

pub fn supported_locales() -> Vec<String> {
    SUPPORTED_LOCALES.iter().map(|s| s.to_string()).collect()
}

static UI_SHELL_DOCKER: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(Some("ui_header_docker"), true, true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_CRATES: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse — Crates")
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(Some("ui_header_crates"), true, true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(Some("ui_header_home"), true, true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_FILES: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse — Files")
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(Some("ui_header_files"), true, true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_ARTIFACTS: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse — Artifacts")
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(Some("ui_header_artifacts"), true, true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
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
