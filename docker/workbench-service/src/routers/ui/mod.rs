use actix_web::dev::HttpServiceFactory;
use actix_web::{HttpResponse, Responder, get, web};
pub use common::assets;
use quench_auth::prelude::JwtConfig;
use quench_starter::prelude::with_base_path;

mod common;
mod pages;

#[get("")]
async fn root(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config).await {
        return common::ui_login_redirect();
    }
    HttpResponse::Found()
        .append_header(("Location", with_base_path("/ui/home")))
        .finish()
}

#[get("/")]
async fn root_slash(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    if !common::is_ui_authenticated(&req, &config).await {
        return common::ui_login_redirect();
    }
    HttpResponse::Found()
        .append_header(("Location", with_base_path("/ui/home")))
        .finish()
}

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/ui")
        // Root
        .service(root)
        .service(root_slash)
        .service(assets)
        // Auth - delegated to gatehouse
        .service(pages::auth::login)
        .service(pages::auth::login_slash)
        .service(pages::auth::callback)
        .service(pages::auth::logout)
        .service(pages::auth::status)
        .service(pages::auth::refresh)
        // Home: the project list
        .service(pages::home::home)
        .service(pages::home::home_slash)
        .service(pages::home::create_project)
        // A project's board
        .service(pages::projects::board)
        .service(pages::projects::create_issue)
        .service(pages::projects::transition_issue)
        // An issue's detail page
        .service(pages::issues::detail)
        .service(pages::issues::update)
        .service(pages::issues::create_comment)
        .service(pages::issues::add_link)
        .service(pages::issues::remove_link)
}
