use crate::actix::domain::auth::UserDb;
use crate::actix::domain::jwt::JwtConfig;
use crate::actix::domain::session::SessionDb;
use crate::actix::routers::auth::{access_cookie, issue_token_pair, refresh_cookie};
use actix_web::{
    HttpResponse,
    cookie::{Cookie, SameSite},
    web,
};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
pub struct LoginQuery {
    pub err: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

static BASE_PATH: LazyLock<String> = LazyLock::new(|| {
    let raw = envmnt::get_or("BASE_PATH", "/");
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        let without_trailing = trimmed.trim_end_matches('/');
        if without_trailing.is_empty() {
            "/".to_string()
        } else if without_trailing.starts_with('/') {
            without_trailing.to_string()
        } else {
            format!("/{without_trailing}")
        }
    }
});

fn ui_path(path: &str) -> String {
    let base = BASE_PATH.as_str();
    let result = if base == "/" {
        format!("/ui{path}")
    } else {
        format!("{}/ui{path}", base)
    };
    tracing::trace!("auth::ui_path({}) -> {} (BASE_PATH={})", path, result, base);
    result
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
    user_db: web::Data<std::sync::Arc<UserDb>>,
    session_db: web::Data<std::sync::Arc<SessionDb>>,
) -> HttpResponse {
    tracing::info!(
        "LOGIN_SUBMIT: Attempting login for username: {}",
        form.username
    );

    if !config.auth_enabled {
        tracing::warn!("LOGIN_SUBMIT: Auth is disabled, redirecting to home");
        return HttpResponse::Found()
            .append_header(("Location", ui_path("/home")))
            .finish();
    }

    let Some(user) = user_db.validate(&form.username, &form.password).await else {
        tracing::warn!(
            "LOGIN_SUBMIT: Invalid credentials for username: {}",
            form.username
        );
        let error_url = ui_path("/login?err=1");
        tracing::debug!("LOGIN_SUBMIT: Redirecting to: {}", error_url);
        return HttpResponse::Found()
            .append_header(("Location", error_url))
            .finish();
    };

    tracing::info!(
        "LOGIN_SUBMIT: User validated successfully: {}",
        user.username
    );

    let Ok(tokens) = issue_token_pair(&config, &session_db, &user).await else {
        tracing::error!(
            "LOGIN_SUBMIT: Failed to issue token pair for user: {}",
            user.username
        );
        let error_url = ui_path("/login?err=1");
        tracing::debug!("LOGIN_SUBMIT: Redirecting to: {}", error_url);
        return HttpResponse::Found()
            .append_header(("Location", error_url))
            .finish();
    };

    tracing::info!("LOGIN_SUBMIT: Tokens issued for user: {}", user.username);
    let access_cookie = access_cookie(&config, tokens.access_token);
    let refresh_cookie = refresh_cookie(&config, tokens.refresh_token);
    let home_url = ui_path("/home");
    tracing::debug!("LOGIN_SUBMIT: Redirecting to home: {}", home_url);

    HttpResponse::Found()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .append_header(("Location", home_url))
        .finish()
}

pub async fn handle_logout(
    request: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    session_db: web::Data<std::sync::Arc<SessionDb>>,
) -> HttpResponse {
    tracing::info!(
        "LOGOUT: User logging out from service: {}",
        config.service_name
    );

    let refresh_cookie_name = format!("{}_refresh_token", config.service_name);
    if let Some(cookie) = request.cookie(&refresh_cookie_name) {
        tracing::debug!("LOGOUT: Revoking refresh token");
        let result = session_db.revoke_by_refresh_token(cookie.value()).await;
        tracing::debug!("LOGOUT: Revoke result: {:?}", result);
    } else {
        tracing::debug!("LOGOUT: No refresh token found in cookies");
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

    let login_url = ui_path("/login");
    tracing::debug!("LOGOUT: Redirecting to: {}", login_url);

    HttpResponse::Found()
        .cookie(access_cookie)
        .cookie(refresh_cookie)
        .append_header(("Location", login_url))
        .finish()
}
