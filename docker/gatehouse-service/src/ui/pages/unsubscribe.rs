//! Ending one kind of notification, from the link in the email.
//!
//! Works without signing in: the link is the credential. Opening it only asks;
//! a button (or a mail provider's one-click POST, RFC 8058) does it - and since
//! turning a subscription off is idempotent, doing it twice is fine.

use super::reset::{render_auth_page, render_auth_page_with};
use crate::notify::catalog::{self, Template};
use crate::notify::prefs::Preferences;
use crate::notify::unsubscribe::Unsubscribe;
use crate::tokens::{PURPOSE_UNSUBSCRIBE, VerificationTokens};
use crate::ui::common::ui_path;
use http::StatusCode;
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Query, Response, get, post};
use quench_web::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Default)]
pub struct TokenQuery {
    #[serde(default)]
    pub token: Option<String>,
}

/// The person and kind a token stands for, if it is genuine and the kind still exists.
async fn resolve(tokens: &VerificationTokens, token: &str) -> Option<(String, &'static Template)> {
    let raw = tokens
        .peek(PURPOSE_UNSUBSCRIBE, token)
        .await
        .unwrap_or(None)?;
    let ticket = Unsubscribe::decode(&raw)?;
    let template = catalog::find(&ticket.template)?;
    Some((ticket.username, template))
}

#[get("/ui/unsubscribe")]
pub async fn unsubscribe_page(
    Query(query): Query<TokenQuery>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
    let token = query.token.unwrap_or_default();
    match resolve(&tokens, &token).await {
        Some((_, template)) => render_unsubscribe_page(&token, template),
        None => render_message_page(StatusCode::NOT_FOUND, "ui_unsubscribe_invalid"),
    }
}

#[post("/ui/unsubscribe")]
pub async fn unsubscribe_submit(
    Query(query): Query<TokenQuery>,
    Form(form): Form<HashMap<String, String>>,
    Inject(db): Inject<Db>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
    // A mail provider's one-click POST carries the token in the URL; the button
    // on our own page does too, so a hidden field is only a fallback.
    let token = query
        .token
        .or_else(|| form.get("token").cloned())
        .unwrap_or_default();
    let Some((username, template)) = resolve(&tokens, &token).await else {
        return render_message_page(StatusCode::NOT_FOUND, "ui_unsubscribe_invalid");
    };
    match Preferences::new(&db).set(&username, template, false).await {
        Ok(()) => render_message_page(StatusCode::OK, "ui_unsubscribe_done"),
        Err(err) => {
            tracing::error!(
                "could not unsubscribe {username} from {}: {err}",
                template.id
            );
            render_message_page(StatusCode::INTERNAL_SERVER_ERROR, "ui_admin_error_internal")
        }
    }
}

pub fn render_unsubscribe_page(token: &str, template: &Template) -> Response {
    let confirm = form()
        .attr("method", "post")
        .attr(
            "action",
            format!(
                "{}?token={}",
                ui_path("/unsubscribe"),
                urlencoding::encode(token)
            ),
        )
        .child(
            element("input")
                .attr("type", "hidden")
                .attr("name", "unsubscribe")
                .attr("value", "1"),
        )
        .child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_unsubscribe_hint"),
        )
        .child(p().attr("data-i18n", template.label_key()))
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_unsubscribe_submit"),
        );
    render_auth_page("ui_unsubscribe_title", confirm)
}

fn render_message_page(status: StatusCode, key: &'static str) -> Response {
    render_auth_page_with(
        status,
        "ui_unsubscribe_title",
        div().child(p().attr("data-i18n", key)),
    )
}

pub fn register_routes() {
    let _ = unsubscribe_page as fn(_, _) -> _;
    let _ = unsubscribe_submit as fn(_, _, _, _) -> _;
}
