use crate::shared::jwt::JwtConfig;
use actix_web::dev::HttpServiceFactory;
use actix_web::{HttpResponse, Responder, get, web};
use serde::Deserialize;

mod common;
mod pages;

pub use common::assets;

#[derive(Deserialize)]
pub(super) struct PageQuery {
    pub(super) repo: Option<String>,
    pub(super) tag: Option<String>,
}

#[get("")]
async fn root(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::PermanentRedirect()
        .append_header(("Location", "/ui/docker/catalog"))
        .finish()
}

#[get("/")]
async fn root_slash(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::PermanentRedirect()
        .append_header(("Location", "/ui/docker/catalog"))
        .finish()
}

#[get("/docker")]
async fn docker_root(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config) {
        return common::ui_login_redirect();
    }
    HttpResponse::PermanentRedirect()
        .append_header(("Location", "/ui/docker/catalog"))
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
        .append_header(("Location", "/ui/docker/catalog"))
        .finish()
}

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/ui")
        .service(root)
        .service(root_slash)
        .service(docker_root)
        .service(docker_root_slash)
        .service(pages::auth::login)
        .service(pages::auth::login_slash)
        .service(pages::auth::login_submit)
        .service(pages::auth::logout)
        .service(pages::catalog::docker_catalog)
        .service(pages::catalog::docker_catalog_slash)
        .service(pages::tags::docker_tags)
}
