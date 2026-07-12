use actix_web::dev::HttpServiceFactory;
use actix_web::{HttpResponse, Responder, get, web};
pub use common::assets;
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::prelude::JwtConfig;
use quench_starter::prelude::with_base_path;

pub mod chat;
pub mod common;
pub mod context_builder;
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

pub fn scope(jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("/ui")
        // Root
        .service(root)
        .service(root_slash)
        .service(assets)
        // Auth (public - no auth required)
        .service(pages::auth::login)
        .service(pages::auth::login_slash)
        .service(pages::auth::login_submit)
        .service(pages::auth::logout)
        .service(pages::auth::auth_status)
        // Chat (with auth)
        .service(chat::scope().wrap(Auth::new(jwt_config.clone())))
        // Projects (with auth)
        .service(pages::projects::scope().wrap(Auth::new(jwt_config.clone())))
        // Files (with auth)
        .service(pages::files::scope().wrap(Auth::new(jwt_config.clone())))
        // Home (with auth)
        .service(pages::home::home)
        .service(pages::home::home_slash)
}
