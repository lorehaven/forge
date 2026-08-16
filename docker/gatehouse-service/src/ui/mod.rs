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
        .service(pages::auth::login_mfa)
        .service(pages::auth::login_mfa_submit)
        .service(pages::auth::logout)
        .service(pages::auth::status)
        .service(pages::auth::refresh)
        .service(pages::register::register_page)
        .service(pages::register::register_page_slash)
        .service(pages::register::register_submit)
        .service(pages::register::verify)
        .service(pages::reset::forgot_password_page)
        .service(pages::reset::forgot_password_page_slash)
        .service(pages::reset::forgot_password_submit)
        .service(pages::reset::reset_password_page)
        .service(pages::reset::reset_password_submit)
        // Requires a realm session; `handle_home` redirects when there is none.
        .service(pages::home::home)
        .service(pages::home::home_slash)
        // Self-service account page - any signed-in user, no catalog action.
        .service(pages::account::account_page)
        .service(pages::account::save_account)
        .service(pages::account::mfa_enroll_page)
        .service(pages::account::mfa_enroll_submit)
        .service(pages::account::mfa_disable)
        // Requires the admin role on top of a session; each handler checks, so a
        // route added here without the check is a compile-time-visible omission
        // rather than an open page.
        .service(pages::admin::users_page)
        .service(pages::admin::users_page_slash)
        .service(pages::admin::create_user)
        // Before `/admin/users/{username}`: actix matches in registration order,
        // and `{username}` would otherwise swallow the delete/template paths'
        // parent.
        .service(pages::admin::delete_user)
        .service(pages::admin::disable_user)
        .service(pages::admin::enable_user)
        .service(pages::admin::unlock_user)
        .service(pages::admin::disable_user_mfa)
        .service(pages::admin::apply_template)
        .service(pages::admin::save_user)
        .service(pages::admin::edit_user)
}
