//! Login and logout belong to gatehouse; this service only hands the browser
//! over. There is deliberately no local login form - gatehouse owns the
//! credentials, the session and the realm cookie.

use actix_web::{HttpRequest, Responder, get, web};
use quench_auth::actix::domain::sso_client::SsoConfig;
use quench_auth::actix::routers::ui::pages::auth::{
    auth_callback, auth_status, login_delegation, logout_delegation,
};
use quench_auth::prelude::JwtConfig;

#[get("/login")]
pub(super) async fn login(req: HttpRequest, sso: web::Data<SsoConfig>) -> impl Responder {
    login_delegation(&req, &sso)
}

#[get("/login/")]
pub(super) async fn login_slash(req: HttpRequest, sso: web::Data<SsoConfig>) -> impl Responder {
    login_delegation(&req, &sso)
}

#[get("/auth/callback")]
pub(super) async fn callback(req: HttpRequest, sso: web::Data<SsoConfig>) -> impl Responder {
    auth_callback(&req, &sso).await
}

#[get("/logout")]
pub(super) async fn logout(req: HttpRequest) -> impl Responder {
    logout_delegation(&req)
}

/// What the page shell's session watcher polls. Shared rather than written
/// here: three services already carry a copy of this, and a fourth that drifted
/// would be a service whose pages stopped noticing a logout.
#[get("/status")]
pub(super) async fn status(req: HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    auth_status(&req, &config).await
}
