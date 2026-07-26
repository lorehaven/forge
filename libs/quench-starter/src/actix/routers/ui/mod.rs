use crate::prelude::with_base_path;
use actix_web::{HttpResponse, web};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub mod common;
pub mod pages;

pub fn ui_path(path: &str) -> String {
    let result = with_base_path(&format!("/ui{path}"));
    tracing::debug!("ui_path({}) = {}", path, result);
    result
}

pub fn ui_asset_path(path: &str) -> String {
    ui_path(&format!("/assets{path}"))
}

pub fn ui_login_redirect() -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", ui_path("/login")))
        .finish()
}

/// Every service puts its pages under `/ui`, so the bare server root and the
/// bare base path land there rather than on a 404. `/ui` itself then decides
/// between the home page and the login page.
fn ui_root_redirect() -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", ui_path("")))
        .finish()
}

#[actix_web::get("/")]
pub async fn server_root_redirect() -> HttpResponse {
    ui_root_redirect()
}

#[actix_web::get("")]
pub async fn base_path_redirect() -> HttpResponse {
    ui_root_redirect()
}

#[actix_web::get("/")]
pub async fn base_path_slash_redirect() -> HttpResponse {
    ui_root_redirect()
}

/// Re-exported rather than reimplemented: a second copy of this check is how
/// the cookie name and audience rule drifted apart in the first place.
pub use quench_auth::actix::routers::ui::is_ui_authenticated;

pub async fn serve_assets(path: web::Path<String>, dist_path: &str) -> HttpResponse {
    let Some(relative) = sanitize_asset_path(&path) else {
        return HttpResponse::BadRequest().finish();
    };

    let full_path = Path::new(dist_path).join(relative);
    let Ok(body) = fs::read(&full_path) else {
        return HttpResponse::NotFound().finish();
    };

    let content_type = content_type_for_path(&full_path);
    HttpResponse::Ok()
        .append_header(("Cache-Control", "public, max-age=3600"))
        .content_type(content_type)
        .body(body)
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
