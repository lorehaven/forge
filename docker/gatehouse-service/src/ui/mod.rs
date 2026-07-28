use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
pub use common::assets;
use quench_auth::prelude::JwtConfig;
use quench_starter::prelude::with_base_path;

pub mod common;
mod pages;

/// `/ui` is where the root redirects land: the service list when there is a
/// session, the login form when there is not.
async fn ui_root(req: &HttpRequest, config: &JwtConfig) -> HttpResponse {
    if common::is_ui_authenticated(req, config).await {
        HttpResponse::Found()
            .append_header(("Location", with_base_path("/ui/home")))
            .finish()
    } else {
        pages::auth::login_redirect()
    }
}

#[get("")]
async fn root(req: HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    ui_root(&req, &config).await
}

#[get("/")]
async fn root_slash(req: HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    ui_root(&req, &config).await
}

pub fn scope() -> actix_web::Scope {
    web::scope("/ui")
        .service(root)
        .service(root_slash)
        .service(assets)
        // Public
        .service(pages::auth::login)
        .service(pages::auth::login_slash)
        .service(pages::auth::login_submit)
        .service(pages::auth::logout)
        .service(pages::auth::status)
        // Requires a realm session; `handle_home` redirects when there is none.
        .service(pages::home::home)
        .service(pages::home::home_slash)
}
