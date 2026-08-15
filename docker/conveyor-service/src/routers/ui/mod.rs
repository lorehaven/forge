use actix_web::dev::HttpServiceFactory;
use actix_web::{HttpResponse, Responder, get, web};
pub use common::assets;
use quench_auth::prelude::JwtConfig;
use quench_starter::prelude::with_base_path;

pub mod common;
pub mod pages;

// ---------------------------------------------------------------------------
// Root redirects
// ---------------------------------------------------------------------------

async fn ui_root(req: &actix_web::HttpRequest, config: &JwtConfig) -> HttpResponse {
    if !common::is_ui_authenticated(req, config).await {
        return common::ui_login_redirect();
    }
    HttpResponse::Found()
        .append_header(("Location", with_base_path("/ui/home")))
        .finish()
}

#[get("")]
async fn root(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    ui_root(&req, &config).await
}

#[get("/")]
async fn root_slash(req: actix_web::HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    ui_root(&req, &config).await
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

pub fn scope(_jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("/ui")
        // Root
        .service(root)
        .service(root_slash)
        .service(assets)
        // Auth: public, because these only hand the browser to gatehouse.
        .service(pages::auth::login)
        .service(pages::auth::login_slash)
        .service(pages::auth::callback)
        .service(pages::auth::logout)
        .service(pages::auth::auth_status)
        .service(pages::auth::refresh)
        // Home checks the session itself and redirects when there is none.
        .service(pages::home::home)
        .service(pages::home::home_slash)
        .service(pages::home::home_state)
        .service(pages::home::run_now)
        .service(pages::home::project_page)
        .service(pages::pipelines::runs_list_page)
        .service(pages::runs::run_page)
        .service(pages::runs::run_state)
        .service(pages::jobs::log)
        .service(pages::scan::scan_page)
        .service(pages::scan::scan_detail_page)
        .service(pages::repos::list_page)
        .service(pages::repos::create_repo)
        .service(pages::repos::edit_page)
        .service(pages::repos::save_repo)
        .service(pages::repos::delete_repo)
        .service(pages::credentials::list_page)
}
