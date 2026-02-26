use crate::domain::jwt::JwtConfig;
use actix_web::{HttpResponse, Responder, get, http::header::ContentType, web};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use quench::prelude::*;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

pub(super) const UI_SESSION_COOKIE: &str = "warehouse_ui_session";

static UI_SHELL_DOCKER: LazyLock<AppShell> = LazyLock::new(|| {
    ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_docker"), true))
        .links(vec![Link::new(
            "stylesheet",
            "/ui/assets/css/warehouse.css",
        )])
        .with_nav(false)
        .resources_prefix("/ui".to_string())
        .build()
});

static UI_SHELL_CRATES: LazyLock<AppShell> = LazyLock::new(|| {
    ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse — Crates")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_crates"), true))
        .links(vec![Link::new(
            "stylesheet",
            "/ui/assets/css/warehouse.css",
        )])
        .with_nav(false)
        .resources_prefix("/ui".to_string())
        .build()
});

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| {
    ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_home"), true))
        .links(vec![Link::new(
            "stylesheet",
            "/ui/assets/css/warehouse.css",
        )])
        .with_nav(false)
        .resources_prefix("/ui".to_string())
        .build()
});

static UI_SHELL_AUTH: LazyLock<AppShell> = LazyLock::new(|| {
    ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(None, false))
        .links(vec![Link::new(
            "stylesheet",
            "/ui/assets/css/warehouse.css",
        )])
        .with_nav(false)
        .resources_prefix("/ui".to_string())
        .build()
});

fn ui_header(title_key: Option<&str>, show_logout: bool) -> Element {
    let title = match title_key {
        Some(key) => h2().attr("data-i18n", key),
        None => h2().attr("data-i18n", "header_label"),
    };

    header()
        .child(div().class("left-panel").child(title))
        .child(div().class("right-panel").child_opt(show_logout.then(|| {
            a().attr("href", "/ui/logout")
                .class("button")
                .attr("data-i18n", "ui_logout")
        })))
}

fn ensure_warehouse_css() {
    let css = warehouse_css_rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");

    let _ = fs::create_dir_all("dist/assets/css");
    let _ = fs::write("dist/assets/css/warehouse.css", css);
}

fn warehouse_css_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".content")
            .property("overflow-y", "hidden")
            .property("padding", "1rem"),
        CssRule::new(".content-inner")
            .property("min-height", "unset")
            .property("width", "100%")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("justify-content", "flex-start")
            .property("align-items", "flex-start")
            .property("padding", "0.5rem"),
        CssRule::new(".page")
            .property("width", "100%")
            .property("flex", "1 1 auto")
            .child(
                CssRule::new(".page-header")
                    .property("height", "5rem")
                    .property("display", "flex")
                    .property("justify-content", "space-between")
                    .property("align-items", "center"),
            )
            .child(
                CssRule::new(".split-view")
                    .property("display", "grid")
                    .property("grid-template-columns", "minmax(20rem, 28rem) minmax(0, 1fr)")
                    .property("gap", "1rem")
                    .property("height", "calc(100vh - 10rem)"),
            )
            .child(
                CssRule::new("@media screen and (max-width: 1024px)")
                    .child(CssRule::new(".split-view").property("grid-template-columns", "1fr")),
            ),
        CssRule::new("header .right-panel")
            .property("display", "flex")
            .property("align-items", "center")
            .child(CssRule::new("a.button").property("padding", "0.6rem 1rem")),
        CssRule::new(".split-left,\n.split-right").property("min-height", "0"),
        CssRule::new(".split-right")
            .property("display", "grid")
            .property("grid-template-rows", "minmax(0, 1fr) minmax(0, 1fr)")
            .property("gap", "1rem")
            .child(
                CssRule::new("@media screen and (max-width: 1024px)")
                    .child(CssRule::new("&").property("grid-template-rows", "minmax(20rem, auto) minmax(14rem, auto)")),
            ),
        CssRule::new(".panel")
            .property("height", "100%")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.3rem")
            .property("background-color", "var(--bs-gray-900)")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("overflow", "hidden"),
        CssRule::new(".panel-title")
            .property("padding", "0.75rem 1rem")
            .property("font-weight", "600")
            .property("border-bottom", "0.1rem solid var(--bs-gray-700)")
            .property("background-color", "var(--bs-gray-800)"),
        CssRule::new(".tree-scroll")
            .property("flex", "1 1 auto")
            .property("min-height", "0")
            .property("height", "calc(100vh - 14rem)")
            .property("max-height", "calc(100vh - 14rem)")
            .property("overflow", "auto")
            .property("padding", "0.75rem"),
        CssRule::new(".repo-tree,\n.repo-tree ul")
            .property("list-style", "none")
            .property("margin", "0")
            .property("padding-left", "1rem"),
        CssRule::new(".repo-tree").property("padding-left", "0"),
        CssRule::new(".tree-folder")
            .property("cursor", "pointer")
            .property("padding", "0.2rem 0"),
        CssRule::new(".repo-link")
            .property("display", "inline-flex")
            .property("padding", "0.15rem 0.3rem")
            .property("border-radius", "0.2rem")
            .property("text-decoration", "none")
            .property("color", "var(--bs-gray-300)")
            .child(
                CssRule::new("&:hover")
                    .property("background-color", "var(--bs-gray-800)"),
            ),
        CssRule::new(".repo-link.active")
            .property("background-color", "var(--bs-success-900)")
            .property("color", "var(--bs-gray-100)"),
        CssRule::new(".table")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("min-height", "0")
            .property("height", "100%")
            .child(CssRule::new(".header").property("display", "grid"))
            .child(
                CssRule::new(".body")
                    .property("flex", "1 1 auto")
                    .property("overflow", "auto")
                    .property("min-height", "0"),
            ),
        // Docker tags grid
        CssRule::new(".tags-grid")
            .child(CssRule::new(".header,\n.body > .row").property("display", "grid"))
            .child(CssRule::new(".header").property("grid-template-columns", "2fr 2fr 3fr"))
            .child(
                CssRule::new(".body > .row")
                    .property("grid-template-columns", "2fr 2fr 3fr")
                    .child(CssRule::new("&.active").property("background-color", "var(--bs-gray-800)"))
                    .child(
                        CssRule::new("&:not(:last-child)")
                            .property("border-bottom", "0.1rem solid var(--bs-gray-700)"),
                    ),
            )
            .child(
                CssRule::new(".cell")
                    .property("padding", "0.45rem 0.55rem")
                    .property("display", "flex")
                    .property("align-items", "center"),
            ),
        // Crates versions grid  – version | status | checksum
        CssRule::new(".versions-grid")
            .child(CssRule::new(".header,\n.body > .row").property("display", "grid"))
            .child(CssRule::new(".header").property("grid-template-columns", "2fr 1fr 3fr"))
            .child(
                CssRule::new(".body > .row")
                    .property("grid-template-columns", "2fr 1fr 3fr")
                    .child(CssRule::new("&.active").property("background-color", "var(--bs-gray-800)"))
                    .child(
                        CssRule::new("&:not(:last-child)")
                            .property("border-bottom", "0.1rem solid var(--bs-gray-700)"),
                    ),
            )
            .child(
                CssRule::new(".cell")
                    .property("padding", "0.45rem 0.55rem")
                    .property("display", "flex")
                    .property("align-items", "center"),
            ),
        CssRule::new(".tag-link")
            .property("text-decoration", "none")
            .property("color", "var(--bs-gray-300)")
            .child(
                CssRule::new("&:hover")
                    .property("color", "var(--bs-gray-100)")
                    .property("text-decoration", "underline"),
            ),
        CssRule::new(".meta-list")
            .property("padding", "0.75rem 1rem")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem"),
        CssRule::new(".meta-row")
            .property("display", "grid")
            .property("grid-template-columns", "10rem 1fr")
            .property("gap", "0.75rem")
            .property("padding", "0.35rem 0")
            .child(
                CssRule::new("&:not(:last-child)")
                    .property("border-bottom", "0.1rem solid var(--bs-gray-800)"),
            ),
        CssRule::new(".meta-label").property("color", "var(--bs-gray-500)"),
        CssRule::new(".mono").property("font-family", "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, \"Liberation Mono\", \"Courier New\", monospace"),
        CssRule::new(".empty")
            .property("padding", "1rem")
            .property("color", "var(--bs-gray-500)"),
        // Dependency display within metadata panel
        CssRule::new(".meta-deps")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem"),
        CssRule::new(".deps-group")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.15rem"),
        CssRule::new(".deps-group-label")
            .property("font-size", "0.75rem")
            .property("text-transform", "uppercase")
            .property("letter-spacing", "0.05em")
            .property("color", "var(--bs-gray-500)")
            .property("margin-bottom", "0.2rem"),
        CssRule::new(".dep-row")
            .property("font-size", "0.85rem")
            .property("color", "var(--bs-gray-300)")
            .property("padding", "0.1rem 0"),
        // Home / service index
        CssRule::new(".home-layout")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "2rem")
            .property("max-width", "56rem")
            .property("margin", "0 auto")
            .property("padding-top", "3rem"),
        CssRule::new(".home-header")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.4rem"),
        CssRule::new(".home-subtitle")
            .property("color", "var(--bs-gray-500)")
            .property("margin", "0"),
        CssRule::new(".home-grid")
            .property("display", "grid")
            .property("grid-template-columns", "repeat(auto-fill, minmax(18rem, 1fr))")
            .property("gap", "1rem"),
        CssRule::new(".home-card")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "space-between")
            .property("padding", "1.25rem 1.5rem")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.4rem")
            .property("background-color", "var(--bs-gray-900)")
            .property("text-decoration", "none")
            .property("color", "inherit")
            .property("transition", "border-color 0.15s, background-color 0.15s")
            .child(
                CssRule::new("&:hover")
                    .property("border-color", "var(--bs-gray-500)")
                    .property("background-color", "var(--bs-gray-800)"),
            ),
        CssRule::new(".home-card-body")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.35rem"),
        CssRule::new(".home-card-title")
            .property("font-size", "1.05rem")
            .property("font-weight", "600")
            .property("color", "var(--bs-gray-100)"),
        CssRule::new(".home-card-desc")
            .property("font-size", "0.85rem")
            .property("color", "var(--bs-gray-400)"),
        CssRule::new(".home-card-arrow")
            .property("font-size", "1.25rem")
            .property("color", "var(--bs-gray-500)")
            .property("flex-shrink", "0")
            .property("padding-left", "1rem"),
        // Login
        CssRule::new(".login-layout")
            .property("min-height", "calc(100vh - 10rem)")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "center"),
        CssRule::new(".login-panel")
            .property("width", "100%")
            .property("max-width", "28rem"),
    ]
}

#[get("/assets/{path:.*}")]
pub async fn assets(path: web::Path<String>) -> impl Responder {
    let Some(relative) = sanitize_asset_path(&path) else {
        return HttpResponse::BadRequest().finish();
    };

    let full_path = Path::new("dist/assets").join(relative);
    let Ok(body) = fs::read(&full_path) else {
        return HttpResponse::NotFound().finish();
    };

    let content_type = content_type_for_path(&full_path);
    HttpResponse::Ok().content_type(content_type).body(body)
}

pub(super) fn render_page(
    mut builder: actix_web::HttpResponseBuilder,
    content: Element,
    page_kind: UiPageKind,
) -> HttpResponse {
    let shell = match page_kind {
        UiPageKind::Home => &*UI_SHELL_HOME,
        UiPageKind::Docker => &*UI_SHELL_DOCKER,
        UiPageKind::Crates => &*UI_SHELL_CRATES,
        UiPageKind::Auth => &*UI_SHELL_AUTH,
    };
    builder
        .content_type(ContentType::html())
        .body(shell.page(div().class("page").child(content)))
}

pub(super) enum UiPageKind {
    Home,
    Docker,
    Crates,
    Auth,
}

pub(super) fn ui_login_redirect() -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", "/ui/login"))
        .finish()
}

pub(super) fn is_ui_authenticated(req: &actix_web::HttpRequest, config: &JwtConfig) -> bool {
    if !config.auth_enabled {
        return true;
    }

    let Some(username) = config.username.as_deref() else {
        return false;
    };
    let Some(password) = config.password.as_deref() else {
        return false;
    };

    let Some(cookie) = req.cookie(UI_SESSION_COOKIE) else {
        return false;
    };

    let Ok(decoded) = STANDARD.decode(cookie.value()) else {
        return false;
    };
    let Ok(credentials) = String::from_utf8(decoded) else {
        return false;
    };
    let Some((cookie_user, cookie_pass)) = credentials.split_once(':') else {
        return false;
    };

    cookie_user == username && cookie_pass == password
}

fn sanitize_asset_path(raw: &str) -> Option<PathBuf> {
    if raw.is_empty() {
        return None;
    }

    let candidate = Path::new(raw);
    let mut clean = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            _ => return None,
        }
    }

    Some(clean)
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}
