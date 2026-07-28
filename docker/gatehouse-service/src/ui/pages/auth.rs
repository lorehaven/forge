//! The one login form in the estate.
//!
//! The form, the credential check and the cookies live here rather than in
//! `quench-auth`: relying parties only ever redirect a browser to this page.

use crate::api::auth::issue_token_pair;
use crate::ui::common::{UiPageKind, render_page, supported_locales, ui_path};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_auth::actix::routers::ui::pages::auth::{
    LoginQuery, redirect_target, validated_redirect,
};
use quench_auth::prelude::realm;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    /// Where to send the browser afterwards. Carried through the form so it
    /// survives the POST; validated before use.
    #[serde(default)]
    pub redirect: Option<String>,
}

#[get("/login")]
pub(super) async fn login(request: HttpRequest, query: web::Query<LoginQuery>) -> impl Responder {
    render_login_page(&request, query.err.as_deref() == Some("1"))
}

#[get("/login/")]
pub(super) async fn login_slash(
    request: HttpRequest,
    query: web::Query<LoginQuery>,
) -> impl Responder {
    render_login_page(&request, query.err.as_deref() == Some("1"))
}

#[post("/login")]
pub(super) async fn login_submit(
    form: web::Form<LoginForm>,
    config: web::Data<JwtConfig>,
    user_db: web::Data<Arc<UserDb>>,
    session_db: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    tracing::info!("login attempt for {}", form.username);

    let Some(user) = user_db.validate(&form.username, &form.password).await else {
        tracing::warn!("invalid credentials for {}", form.username);
        return HttpResponse::Found()
            .append_header(("Location", ui_path("/login?err=1")))
            .finish();
    };

    let Ok(tokens) = issue_token_pair(&config, &session_db, &user).await else {
        tracing::error!("failed to issue tokens for {}", user.username);
        return HttpResponse::Found()
            .append_header(("Location", ui_path("/login?err=1")))
            .finish();
    };

    let target = form
        .redirect
        .as_deref()
        .and_then(validated_redirect)
        .unwrap_or_else(|| ui_path("/home"));

    HttpResponse::Found()
        .cookie(realm::session_cookie(tokens.access_token))
        .cookie(realm::refresh_cookie(tokens.refresh_token))
        .append_header(("Location", target))
        .finish()
}

/// What the page shell's session watcher polls.
///
/// Gatehouse needs this as much as a relying party does: `/ui/home` is the
/// estate's launcher, which is exactly the kind of page somebody leaves open.
/// The watcher does not turn the login page into a redirect loop, because it
/// refuses to redirect from a `/login` path in the first place.
#[get("/status")]
pub(super) async fn status(request: HttpRequest, config: web::Data<JwtConfig>) -> impl Responder {
    quench_auth::actix::routers::ui::pages::auth::auth_status(&request, &config)
}

/// Realm-wide logout: revokes the session and clears the shared cookie, so
/// every service sees the user as signed out.
#[get("/logout")]
pub(super) async fn logout(
    request: HttpRequest,
    session_db: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    if let Some(cookie) = request.cookie(&realm::refresh_cookie_name()) {
        let revoked = session_db.revoke_by_refresh_token(cookie.value()).await;
        tracing::debug!("logout revoke result: {revoked:?}");
    }

    let target = redirect_target(&request).unwrap_or_else(|| ui_path("/login"));

    HttpResponse::Found()
        .cookie(realm::cleared_session_cookie())
        .cookie(realm::cleared_refresh_cookie())
        .append_header(("Location", target))
        .finish()
}

fn render_login_page(request: &HttpRequest, error: bool) -> HttpResponse {
    // Carried through the form so a login that started at sage returns to sage.
    let redirect = redirect_target(request);

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
                .attr("autofocus", "autofocus")
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

    if let Some(target) = redirect.filter(|value| !value.is_empty()) {
        login_form = login_form.child(
            element("input")
                .attr("type", "hidden")
                .attr("name", "redirect")
                .attr("value", target),
        );
    }

    if error {
        login_form = login_form.child(
            p().class("error")
                .attr("data-i18n", "ui_login_invalid_credentials"),
        );
    }

    // The page shell has no top panel here, so the card carries the estate
    // label and the language switch itself, in a bar above the credentials.
    let login_bar = div()
        .class("login-bar")
        .child(
            span()
                .class("login-brand")
                .attr("data-i18n", "header_label"),
        )
        .child(locale_switch(Some(supported_locales()), None));

    let credentials = div()
        .class("login-credentials")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_login_sign_in"),
        )
        .child(div().class("meta-list").child(login_form));

    render_page(
        HttpResponse::Ok(),
        content().class("container-fluid login-layout").child(
            div()
                .class("panel login-panel")
                .child(login_bar)
                .child(credentials),
        ),
        UiPageKind::Auth,
    )
}

/// Anything under `/ui` that is not a page sends you to the login form.
pub(crate) fn login_redirect() -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", ui_path("/login")))
        .finish()
}
