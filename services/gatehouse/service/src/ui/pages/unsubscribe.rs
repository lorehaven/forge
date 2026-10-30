//! Ending one kind of notification, from the link in the email.
//!
//! Works without signing in: the link is the credential. Opening it only asks;
//! a button (or a mail provider's one-click POST, RFC 8058) does it - and since
//! turning a subscription off is idempotent, doing it twice is fine.

use crate::notify::catalog::{self, Template};
use crate::notify::prefs::Preferences;
use crate::notify::unsubscribe::Unsubscribe;
use crate::tokens::{PURPOSE_UNSUBSCRIBE, VerificationTokens};
use crate::ui::common::FormFields;
use quench_db::prelude::Db;
use quench_http::http::StatusCode;
use quench_http::prelude::{Form, Inject, Query, Response, get, post};
use quench_serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct TokenQuery {
    #[serde(default)]
    pub token: Option<String>,
}

/// The person and kind a token stands for, if it is genuine and the kind still exists.
pub async fn resolve(
    tokens: &VerificationTokens,
    token: &str,
) -> Option<(String, &'static Template)> {
    let raw = tokens
        .peek(PURPOSE_UNSUBSCRIBE, token)
        .await
        .unwrap_or(None)?;
    let ticket = Unsubscribe::decode(&raw)?;
    let template = catalog::find(&ticket.template)?;
    Some((ticket.username, template))
}

/// The page the link in a notification opens: the client asks `GET /api/v1/ui/unsubscribe` what the link stands for.
#[get("/ui/unsubscribe")]
pub async fn unsubscribe_page(Query(_query): Query<TokenQuery>) -> Response {
    super::client::public()
}

/// Ends the subscription the token stands for.
///
/// The button on the page and a mail provider's one-click POST both come here;
/// the answer is JSON (`{"done":true}`, or `{"error":"<translation key>"}` with the status that says why).
#[post("/ui/unsubscribe")]
pub async fn unsubscribe_submit(
    Query(query): Query<TokenQuery>,
    Form(form): Form<FormFields>,
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
        return answer(StatusCode::NOT_FOUND, "ui_unsubscribe_invalid");
    };
    match Preferences::new(&db).set(&username, template, false).await {
        Ok(()) => Response::json(StatusCode::OK, &quench_serde::json!({ "done": true }))
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(err) => {
            quench_log::error!(
                "could not unsubscribe {username} from {}: {err}",
                template.id
            );
            answer(StatusCode::INTERNAL_SERVER_ERROR, "ui_admin_error_internal")
        }
    }
}

fn answer(status: StatusCode, key: &str) -> Response {
    Response::json(status, &quench_serde::json!({ "error": key }))
        .unwrap_or_else(|_| Response::new(status))
}

pub fn register_routes() {
    let _ = unsubscribe_page as fn(_) -> _;
    let _ = unsubscribe_submit as fn(_, _, _, _) -> _;
}
