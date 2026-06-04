use actix_web::dev::HttpServiceFactory;
use actix_web::{HttpResponse, Responder, get, web};
pub use common::assets;
use quench_srv::prelude::{JwtConfig, with_base_path};

pub mod chat;
pub mod common;
pub mod pages;

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

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/ui")
        // Root
        .service(root)
        .service(root_slash)
        .service(assets)
        // Chat
        .service(chat::scope())
        // Auth
        .service(pages::auth::login)
        .service(pages::auth::login_slash)
        .service(pages::auth::login_submit)
        .service(pages::auth::logout)
        .service(pages::auth::auth_status)
        // Home
        .service(pages::home::home)
        .service(pages::home::home_slash)
}
