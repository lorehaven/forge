//! Login and logout belong to gatehouse; this service only hands the browser
//! over. There is deliberately no local login form - gatehouse owns the
//! credentials, the session and the realm cookie.

use actix_web::{HttpRequest, Responder, get};
use quench_auth::actix::routers::ui::pages::auth::{login_delegation, logout_delegation};

#[get("/login")]
pub(super) async fn login(req: HttpRequest) -> impl Responder {
    login_delegation(&req)
}

#[get("/login/")]
pub(super) async fn login_slash(req: HttpRequest) -> impl Responder {
    login_delegation(&req)
}

#[get("/logout")]
pub(super) async fn logout(req: HttpRequest) -> impl Responder {
    logout_delegation(&req)
}
