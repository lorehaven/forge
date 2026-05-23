use crate::routers::ui::common::{UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, post, web};
use quench_srv::actix::domain::auth::UserDb;
use quench_srv::actix::routers::ui::pages::auth::{
    LoginForm, LoginQuery, handle_login_submit, handle_logout, login_form_element,
};
use quench_srv::prelude::jwt::JwtConfig;
use quench_web::prelude::*;
use serde::Serialize;

#[get("/login")]
pub(super) async fn login(query: web::Query<LoginQuery>) -> impl Responder {
    render_login_page(query.err.as_deref() == Some("1"))
}

#[get("/login/")]
pub(super) async fn login_slash(query: web::Query<LoginQuery>) -> impl Responder {
    render_login_page(query.err.as_deref() == Some("1"))
}

#[post("/login")]
pub(super) async fn login_submit(
    form: web::Form<LoginForm>,
    config: web::Data<JwtConfig>,
    user_db: web::Data<UserDb>,
) -> impl Responder {
    handle_login_submit(form, config, user_db).await
}

#[get("/logout")]
pub(super) async fn logout(config: web::Data<JwtConfig>) -> impl Responder {
    handle_logout(config).await
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
            roles: vec!["Admin".to_string()],
        });
    }

    let cookie_name = format!("{}_ui_session", config.service_name);
    let Some(cookie) = req.cookie(&cookie_name) else {
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

fn render_login_page(error: bool) -> HttpResponse {
    let login_form = login_form_element(error);

    render_page(
        HttpResponse::Ok(),
        content().class("container-fluid login-layout").child(
            div()
                .class("panel login-panel")
                .child(
                    div()
                        .class("panel-title")
                        .attr("data-i18n", "ui_login_sign_in"),
                )
                .child(div().class("meta-list").child(login_form)),
        ),
        UiPageKind::Auth,
    )
}
