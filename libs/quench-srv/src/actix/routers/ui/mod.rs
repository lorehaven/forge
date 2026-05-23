use crate::actix::domain::jwt::JwtConfig;
use crate::prelude::with_base_path;
use actix_web::{HttpResponse, web};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub mod common;
pub mod pages;

pub fn ui_path(path: &str) -> String {
    with_base_path(&format!("/ui{path}"))
}

pub fn ui_asset_path(path: &str) -> String {
    ui_path(&format!("/assets{path}"))
}

pub fn ui_login_redirect() -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", ui_path("/login")))
        .finish()
}

pub fn is_ui_authenticated(req: &actix_web::HttpRequest, config: &JwtConfig) -> bool {
    if !config.auth_enabled {
        return true;
    }

    let cookie_name = format!("{}_ui_session", config.service_name);
    let Some(cookie) = req.cookie(&cookie_name) else {
        return false;
    };

    match config.decode_claims(cookie.value()) {
        Ok(claims) => claims.service == config.service_name,
        Err(_) => false,
    }
}

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
