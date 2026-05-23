use crate::actix::domain::jwt::JwtConfig;
use crate::actix::routers::ui::ui_path;
use crate::prelude::with_base_path;
use actix_web::{
    HttpResponse,
    cookie::{Cookie, SameSite},
    web,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use quench_web::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LoginQuery {
    pub err: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

pub fn login_form_element(error: bool) -> Element {
    let mut login_form = form()
        .attr("method", "post")
        .attr("action", &ui_path("/login"))
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

    login_form
}

pub async fn handle_login_submit(form: web::Form<LoginForm>, config: &JwtConfig) -> HttpResponse {
    if !config.auth_enabled {
        return HttpResponse::Found()
            .append_header(("Location", with_base_path("/ui/home")))
            .finish();
    }

    let Some(expected_user) = config.username.as_deref() else {
        return HttpResponse::Found()
            .append_header(("Location", with_base_path("/ui/login?err=1")))
            .finish();
    };
    let Some(expected_pass) = config.password.as_deref() else {
        return HttpResponse::Found()
            .append_header(("Location", with_base_path("/ui/login?err=1")))
            .finish();
    };

    if form.username != expected_user || form.password != expected_pass {
        return HttpResponse::Found()
            .append_header(("Location", with_base_path("/ui/login?err=1")))
            .finish();
    }

    let session = STANDARD.encode(format!("{}:{}", form.username, form.password));
    let cookie_name = format!("{}_ui_session", config.service_name);
    let cookie = Cookie::build(cookie_name, session)
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(true)
        .finish();

    HttpResponse::Found()
        .cookie(cookie)
        .append_header(("Location", with_base_path("/ui/home")))
        .finish()
}

pub async fn handle_logout(config: &JwtConfig) -> HttpResponse {
    let cookie_name = format!("{}_ui_session", config.service_name);
    let cookie = Cookie::build(cookie_name, "")
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(actix_web::cookie::time::Duration::seconds(0))
        .finish();

    HttpResponse::Found()
        .cookie(cookie)
        .append_header(("Location", with_base_path("/ui/login")))
        .finish()
}
