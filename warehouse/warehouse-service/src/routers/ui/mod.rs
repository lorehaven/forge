use crate::domain::jwt::JwtConfig;
use crate::routers::with_base_path;
use actix_web::dev::HttpServiceFactory;
use actix_web::{HttpResponse, Responder, get, web};
use serde::Deserialize;

mod common;
mod pages;

pub use common::assets;

#[derive(Deserialize)]
pub(super) struct PageQuery {
    /// Selected crate name (or docker repository)
    pub(super) repo: Option<String>,
    /// Selected version (or docker tag)
    pub(super) tag: Option<String>,
}

// ---------------------------------------------------------------------------
// Root redirects
// ---------------------------------------------------------------------------

#[get("")]
async fn root(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::Found()
        .append_header(("Location", with_base_path("/ui/home")))
        .finish()
}

#[get("/")]
async fn root_slash(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::Found()
        .append_header(("Location", with_base_path("/ui/home")))
        .finish()
}

// Docker redirects

#[get("/docker")]
async fn docker_root(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::PermanentRedirect()
        .append_header(("Location", with_base_path("/ui/docker/catalog")))
        .finish()
}

#[get("/docker/")]
async fn docker_root_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::PermanentRedirect()
        .append_header(("Location", with_base_path("/ui/docker/catalog")))
        .finish()
}

// Crates redirects

#[get("/crates")]
async fn crates_root(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::PermanentRedirect()
        .append_header(("Location", with_base_path("/ui/crates/catalog")))
        .finish()
}

// Files redirects

#[get("/files")]
async fn files_root(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::PermanentRedirect()
        .append_header(("Location", with_base_path("/ui/files/catalog")))
        .finish()
}

#[get("/files/")]
async fn files_root_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::PermanentRedirect()
        .append_header(("Location", with_base_path("/ui/files/catalog")))
        .finish()
}

#[get("/crates/")]
async fn crates_root_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::PermanentRedirect()
        .append_header(("Location", with_base_path("/ui/crates/catalog")))
        .finish()
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/ui")
        // Root
        .service(root)
        .service(root_slash)
        .service(assets)
        // Docker
        .service(docker_root)
        .service(docker_root_slash)
        // Crates redirects
        .service(crates_root)
        .service(crates_root_slash)
        // Files redirects
        .service(files_root)
        .service(files_root_slash)
        // Auth
        .service(pages::auth::login)
        .service(pages::auth::login_slash)
        .service(pages::auth::login_submit)
        .service(pages::auth::logout)
        // Home
        .service(pages::home::home)
        .service(pages::home::home_slash)
        // Docker pages
        .service(pages::docker::catalog::docker_catalog)
        .service(pages::docker::catalog::docker_catalog_slash)
        .service(pages::docker::tags::docker_tags)
        // Crates pages
        .service(pages::crates::catalog::crates_index)
        .service(pages::crates::catalog::crates_index_slash)
        // Files pages
        .service(pages::files::catalog::files_catalog)
        .service(pages::files::catalog::files_catalog_slash)
}
