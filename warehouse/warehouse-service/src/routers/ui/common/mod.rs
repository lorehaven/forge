use crate::routers::with_base_path;
use actix_web::{HttpResponse, Responder, get, http::header::ContentType, web};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use quench_srv::prelude::jwt::JwtConfig;
use quench_web::prelude::*;
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::LazyLock,
};

mod crates_js;
mod docker_js;
mod files_js;
mod warehouse_css;

pub(super) const UI_SESSION_COOKIE: &str = "warehouse_ui_session";

static UI_SHELL_DOCKER: LazyLock<AppShell> = LazyLock::new(|| {
    warehouse_css::ensure_warehouse_css();
    docker_js::ensure_docker_js();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_docker"), true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .scripts(vec![Script::new(&ui_asset_path("/js/docker.js"))])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_CRATES: LazyLock<AppShell> = LazyLock::new(|| {
    warehouse_css::ensure_warehouse_css();
    crates_js::ensure_crates_js();

    AppShellBuilder::new()
        .title("Warehouse — Crates")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_crates"), true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .scripts(vec![Script::new(&ui_asset_path("/js/crates.js"))])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_FILES: LazyLock<AppShell> = LazyLock::new(|| {
    warehouse_css::ensure_warehouse_css();
    files_js::ensure_files_js();

    AppShellBuilder::new()
        .title("Warehouse — Files")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_files"), true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .scripts(vec![Script::new(&ui_asset_path("/js/files.js"))])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| {
    warehouse_css::ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(Some("ui_header_home"), true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_AUTH: LazyLock<AppShell> = LazyLock::new(|| {
    warehouse_css::ensure_warehouse_css();

    AppShellBuilder::new()
        .title("Warehouse")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark])
        .header(ui_header(None, false, false))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/warehouse.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

fn ui_header(title_key: Option<&str>, show_home: bool, show_logout: bool) -> Element {
    let title = match title_key {
        Some(key) => h2().attr("data-i18n", key),
        None => h2().attr("data-i18n", "header_label"),
    };

    header()
        .child(div().class("left-panel").child(title))
        .child(
            div()
                .class("right-panel")
                .child_opt(show_home.then(|| {
                    a().attr("href", &ui_path("/home"))
                        .class("button")
                        .attr("data-i18n", "ui_home_button")
                }))
                .child_opt(show_logout.then(|| {
                    a().attr("href", &ui_path("/logout"))
                        .class("button")
                        .attr("data-i18n", "ui_logout")
                })),
        )
}

pub(super) fn ui_path(path: &str) -> String {
    with_base_path(&format!("/ui{path}"))
}

pub(super) fn ui_asset_path(path: &str) -> String {
    ui_path(&format!("/assets{path}"))
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
    HttpResponse::Ok()
        .append_header(("Cache-Control", "public, max-age=3600"))
        .content_type(content_type)
        .body(body)
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
        UiPageKind::Files => &*UI_SHELL_FILES,
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
    Files,
    Auth,
}

pub(super) fn ui_login_redirect() -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", with_base_path("/ui/login")))
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
