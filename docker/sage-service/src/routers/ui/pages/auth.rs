//! Login and logout belong to gatehouse; this service only hands the browser
//! over. There is deliberately no local login form - gatehouse owns the
//! credentials, the session and the realm cookie.

use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench_auth::actix::routers::ui::pages::auth::{login_delegation, logout_delegation};
use quench_auth::prelude::JwtConfig;
use serde::Serialize;

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

#[derive(Serialize)]
struct AuthStatus {
    authenticated: bool,
    username: Option<String>,
    roles: Vec<String>,
}

#[get("/status")]
pub(super) async fn auth_status(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !config.auth_enabled {
        return HttpResponse::Ok().json(AuthStatus {
            authenticated: true,
            username: Some("dev".to_string()),
            roles: vec!["admin".to_string()],
        });
    }

    let Some(cookie) = req.cookie(&quench_auth::prelude::realm::session_cookie_name()) else {
        return HttpResponse::Ok().json(AuthStatus {
            authenticated: false,
            username: None,
            roles: vec![],
        });
    };

    match config.decode_claims(cookie.value()) {
        Ok(claims) => HttpResponse::Ok().json(AuthStatus {
            authenticated: true,
            username: Some(claims.sub),
            roles: claims
                .scope
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
        }),
        Err(_) => HttpResponse::Ok().json(AuthStatus {
            authenticated: false,
            username: None,
            roles: vec![],
        }),
    }
}
