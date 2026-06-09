use crate::actix::domain::auth::UserDb;
use crate::actix::domain::jwt::JwtConfig;
use crate::actix::domain::session::SessionDb;
use crate::actix::routers::auth::{access_cookie, issue_token_pair, refresh_cookie};
use crate::actix::routers::ui::ui_path;
use crate::prelude::with_base_path;
use actix_web::{
    HttpResponse,
    cookie::{Cookie, SameSite},
    web,
};
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
        .attr("action", ui_path("/login"))
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

pub async fn handle_login_submit(
    form: web::Form<LoginForm>,
    config: web::Data<JwtConfig>,
    user_db: web::Data<UserDb>,
    session_db: web::Data<SessionDb>,
) -> HttpResponse {
    if !config.auth_enabled {
        return HttpResponse::Found()
            .append_header(("Location", with_base_path("/ui/home")))
            .finish();
    }

    let Some(user) = user_db.validate(&form.username, &form.password).await else {
        return HttpResponse::Found()
            .append_header(("Location", with_base_path("/ui/login?err=1")))
            .finish();
    };

    let Ok(tokens) = issue_token_pair(&config, &session_db, &user).await else {
        return HttpResponse::Found()
            .append_header(("Location", with_base_path("/ui/login?err=1")))
            .finish();
    };

    let access_cookie = access_cookie(&config, tokens.access_token);
    let refresh_cookie = refresh_cookie(&config, tokens.refresh_token);

    HttpResponse::Found()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .append_header(("Location", with_base_path("/ui/home")))
        .finish()
}

pub async fn handle_logout(
    request: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    session_db: web::Data<SessionDb>,
) -> HttpResponse {
    let refresh_cookie_name = format!("{}_refresh_token", config.service_name);
    if let Some(cookie) = request.cookie(&refresh_cookie_name) {
        let _ = session_db.revoke_by_refresh_token(cookie.value()).await;
    }

    let cookie_name = format!("{}_ui_session", config.service_name);
    let access_cookie = Cookie::build(cookie_name, "")
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(actix_web::cookie::time::Duration::seconds(0))
        .finish();
    let refresh_cookie = Cookie::build(refresh_cookie_name, "")
        .path("/")
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(actix_web::cookie::time::Duration::seconds(0))
        .finish();

    HttpResponse::Found()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .append_header(("Location", with_base_path("/ui/login")))
        .finish()
}
