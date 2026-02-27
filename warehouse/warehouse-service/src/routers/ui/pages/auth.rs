use crate::domain::jwt::JwtConfig;
use crate::routers::ui::common::{UI_SESSION_COOKIE, UiPageKind, render_page};
use actix_web::cookie::{Cookie, SameSite};
use actix_web::{HttpResponse, Responder, get, post, web};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use quench::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct LoginQuery {
    err: Option<String>,
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

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
) -> impl Responder {
    if !config.auth_enabled {
        return HttpResponse::Found()
            .append_header(("Location", "/ui/home"))
            .finish();
    }

    let Some(expected_user) = config.username.as_deref() else {
        return HttpResponse::Found()
            .append_header(("Location", "/ui/login?err=1"))
            .finish();
    };
    let Some(expected_pass) = config.password.as_deref() else {
        return HttpResponse::Found()
            .append_header(("Location", "/ui/login?err=1"))
            .finish();
    };

    if form.username != expected_user || form.password != expected_pass {
        return HttpResponse::Found()
            .append_header(("Location", "/ui/login?err=1"))
            .finish();
    }

    let session = STANDARD.encode(format!("{}:{}", form.username, form.password));
    let cookie = Cookie::build(UI_SESSION_COOKIE, session)
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(true)
        .finish();

    HttpResponse::Found()
        .cookie(cookie)
        .append_header(("Location", "/ui/home"))
        .finish()
}

#[get("/logout")]
pub(super) async fn logout() -> impl Responder {
    let cookie = Cookie::build(UI_SESSION_COOKIE, "")
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(actix_web::cookie::time::Duration::seconds(0))
        .finish();

    HttpResponse::Found()
        .cookie(cookie)
        .append_header(("Location", "/ui/login"))
        .finish()
}

fn render_login_page(error: bool) -> HttpResponse {
    let mut login_form = form()
        .attr("method", "post")
        .attr("action", "/ui/login")
        .child(
            label()
                .attr("for", "username")
                .attr("data-i18n", "ui_login_username"),
        )
        .child(
            element("input")
                .attr("type", "text")
                .attr("id", "username")
                .attr("name", "username")
                .attr("autocomplete", "username")
                .attr("required", "required"),
        )
        .child(
            label()
                .attr("for", "password")
                .attr("data-i18n", "ui_login_password"),
        )
        .child(
            element("input")
                .attr("type", "password")
                .attr("id", "password")
                .attr("name", "password")
                .attr("autocomplete", "current-password")
                .attr("required", "required"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_login_submit"),
        );

    if error {
        login_form = login_form.child(
            p().class("error")
                .attr("data-i18n", "ui_login_invalid_credentials"),
        );
    }

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
