//! Accepting an invitation: the page the emailed link opens.
//!
//! Choosing a password here also confirms the address, since the link reached it.

use super::reset::render_auth_page;
use crate::realm;
use crate::tokens::{PURPOSE_INVITE, VerificationTokens};
use crate::ui::common::ui_path;
use http::StatusCode;
use quench_auth::domain::session::SessionDb;
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Query, Response, get, post};
use quench_web::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct AcceptQuery {
    pub token: String,
}

#[derive(Deserialize, Default)]
pub struct AcceptNotice {
    #[serde(default)]
    pub err: Option<String>,
}

#[get("/ui/accept-invite")]
pub async fn accept_invite_page(
    Query(query): Query<AcceptQuery>,
    Query(notice): Query<AcceptNotice>,
) -> Response {
    render_accept_invite_page(&query.token, &notice)
}

#[derive(Deserialize)]
pub struct AcceptForm {
    pub token: String,
    pub password: String,
}

#[post("/ui/accept-invite")]
pub async fn accept_invite_submit(
    Form(form): Form<AcceptForm>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
    // Checked before the token is spent, so a blank submission leaves the
    // invitation usable instead of burning it.
    if form.password.trim().is_empty() {
        return redirect(&format!(
            "{}?token={}&err=ui_reset_error_password_empty",
            ui_path("/accept-invite"),
            urlencoding::encode(&form.token)
        ));
    }

    let Some(username) = tokens
        .redeem(PURPOSE_INVITE, &form.token)
        .await
        .unwrap_or(None)
    else {
        return redirect(&ui_path("/login?err=ui_login_invite_invalid"));
    };

    if let Err(err) = realm::reset_password(&db, &sessions, &username, &form.password).await {
        tracing::error!("could not set the password for invited {username}: {err:?}");
        return redirect(&ui_path("/login?err=ui_login_invite_invalid"));
    }
    // The link reached the address on file, so it is confirmed.
    if let Ok(user) = realm::get(&db, &username).await
        && user.email.is_some()
        && user.email_verified_at.is_none()
        && let Err(err) = realm::mark_email_verified(&db, &username).await
    {
        tracing::error!("could not confirm the address of invited {username}: {err:?}");
    }
    redirect(&ui_path("/login?invited=1"))
}

fn redirect(location: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", location)
}

pub fn render_accept_invite_page(token: &str, notice: &AcceptNotice) -> Response {
    let mut accept_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/accept-invite"))
        .child(
            element("input")
                .attr("type", "hidden")
                .attr("name", "token")
                .attr("value", token),
        )
        .child(
            label()
                .attr("for", "password")
                .attr("data-i18n", "ui_reset_new_password"),
        )
        .child(
            input()
                .attr("type", "password")
                .attr("id", "password")
                .attr("name", "password")
                .attr("autocomplete", "new-password")
                .attr("autofocus", "autofocus")
                .attr("required", "required"),
        )
        .child(p().class("admin-hint").attr("data-i18n", "ui_invite_hint"))
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_invite_submit"),
        );

    if notice.err.as_deref() == Some("ui_reset_error_password_empty") {
        accept_form = accept_form.child(
            p().class("error")
                .attr("data-i18n", "ui_reset_error_password_empty"),
        );
    }

    render_auth_page("ui_invite_title", accept_form)
}

pub fn register_routes() {
    let _ = accept_invite_page as fn(_, _) -> _;
    let _ = accept_invite_submit as fn(_, _, _, _) -> _;
}
