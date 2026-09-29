//! Confirming a new email address: the page the emailed link opens.
//!
//! Opening the link only *shows* what would change; a button does it. A link
//! that changed something on GET would fire whenever a mail scanner or link
//! preview fetched it, with nobody at the keyboard.

use super::reset::render_auth_page;
use crate::email::{Mail, Recipient, Sender};
use crate::email_change::Ticket;
use crate::realm;
use crate::tokens::{PURPOSE_EMAIL_CHANGE, VerificationTokens};
use crate::ui::common::ui_path;
use crate::ui::locale::BrowserLocale;
use http::StatusCode;
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Query, Response, get, post};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ConfirmQuery {
    pub token: String,
}

#[get("/ui/confirm-email")]
pub async fn confirm_email_page(
    Query(query): Query<ConfirmQuery>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
    let ticket = tokens
        .peek(PURPOSE_EMAIL_CHANGE, &query.token)
        .await
        .unwrap_or(None)
        .and_then(|raw| Ticket::decode(&raw));
    match ticket {
        Some(ticket) => render_confirm_email_page(&query.token, &ticket.new_email),
        None => redirect(&ui_path("/login?err=ui_login_confirm_email_invalid")),
    }
}

#[derive(Deserialize)]
pub struct ConfirmForm {
    pub token: String,
}

#[post("/ui/confirm-email")]
pub async fn confirm_email_submit(
    Form(form): Form<ConfirmForm>,
    Inject(db): Inject<Db>,
    Inject(tokens): Inject<VerificationTokens>,
    Inject(mailer): Inject<Arc<dyn Sender>>,
    browser_locale: BrowserLocale,
) -> Response {
    let invalid = || redirect(&ui_path("/login?err=ui_login_confirm_email_invalid"));

    let Some(ticket) = tokens
        .redeem(PURPOSE_EMAIL_CHANGE, &form.token)
        .await
        .unwrap_or(None)
        .and_then(|raw| Ticket::decode(&raw))
    else {
        return invalid();
    };

    let change = match realm::change_email(&db, &ticket.username, &ticket.new_email).await {
        Ok(change) => change,
        Err(err) => {
            tracing::warn!("email change for {} failed: {err:?}", ticket.username);
            return invalid();
        }
    };

    // Tell the address that is being replaced, if it had been confirmed: it is
    // the one place a hijacked account's real owner might still be listening.
    if let Some(previous) = &change.previous_confirmed {
        let recipient = Recipient {
            address: previous,
            username: &change.user.username,
            locale: change
                .user
                .preferred_locale
                .as_deref()
                .or(browser_locale.0.as_deref()),
        };
        let notice = Mail::EmailChanged {
            new_email: &ticket.new_email,
        };
        if let Err(err) = mailer.send(&recipient, &notice).await {
            tracing::error!(
                "failed to tell the old address of {} about the change: {err}",
                change.user.username
            );
        }
    }
    redirect(&ui_path("/login?email_changed=1"))
}

fn redirect(location: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", location)
}

pub fn render_confirm_email_page(token: &str, new_email: &str) -> Response {
    let confirm_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/confirm-email"))
        .child(
            element("input")
                .attr("type", "hidden")
                .attr("name", "token")
                .attr("value", token),
        )
        .child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_confirm_email_hint"),
        )
        .child(p().attr("id", "new-email").text(new_email))
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_confirm_email_submit"),
        );
    render_auth_page("ui_confirm_email_title", confirm_form)
}

pub fn register_routes() {
    let _ = confirm_email_page as fn(_, _) -> _;
    let _ = confirm_email_submit as fn(_, _, _, _, _) -> _;
}
