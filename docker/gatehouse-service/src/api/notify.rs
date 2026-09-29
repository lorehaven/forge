//! `POST /api/v1/notify` - a service asking gatehouse to tell someone something.
//!
//! The caller says *what happened* (a template id and its variables) and *to
//! whom* (a username); gatehouse owns everything else - the wording, the
//! language, whether the person wants it, and the unsubscribe link. See
//! [`crate::notify`].

use crate::PublicBase;
use crate::api::users::SubjectClaims;
use crate::email::Sender;
use crate::notify::{self, Context, Outcome, Request as NotifyRequest};
use crate::ratelimit::RateLimiter;
use crate::tokens::VerificationTokens;
use async_trait::async_trait;
use http::StatusCode;
use quench_auth::domain::jwt::Claims;
use quench_db::prelude::Db;
use quench_http::prelude::{FromRequest, HttpError, Inject, Json, Response, post};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A verified token allowed to send notifications: a machine identity (the
/// `service` role is a wildcard) or someone explicitly granted `gatehouse:notify`.
/// An ordinary user's token is refused - this endpoint sends mail as the estate.
pub struct NotifyClaims(pub Claims);

#[async_trait]
impl FromRequest for NotifyClaims {
    async fn from_request(req: &mut quench_http::prelude::Request) -> Result<Self, HttpError> {
        let SubjectClaims(claims) = SubjectClaims::from_request(req).await?;
        if !claims.can("gatehouse", "notify") {
            tracing::warn!(
                "{} lacks gatehouse:notify; refusing to send a notification",
                claims.sub
            );
            return Err(HttpError::status(StatusCode::FORBIDDEN, ""));
        }
        Ok(Self(claims))
    }
}

#[derive(Debug, Deserialize)]
pub struct NotifyBody {
    pub username: String,
    /// A template id from the catalog, e.g. `conveyor.run.failed`.
    pub template: String,
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
    /// Repeating a key (a retry) does not send a second time within a day.
    #[serde(default)]
    pub dedupe_key: Option<String>,
    /// The person subscribed to this in the calling service: sends even a kind
    /// that is off by default, unless they opted out of the kind here.
    #[serde(default)]
    pub requested: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotifyResponse {
    /// `accepted`, `skipped` or `failed`.
    pub status: String,
    /// Why it was skipped, or what went wrong.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// For `failed`: whether trying again later could work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}

#[derive(Serialize)]
struct Problem {
    error: String,
}

// One DI extractor per argument - the framework has no struct-of-extractors.
#[allow(clippy::too_many_arguments)]
#[post("/api/v1/notify")]
async fn notify_user(
    actor: NotifyClaims,
    Json(body): Json<NotifyBody>,
    Inject(db): Inject<Db>,
    Inject(tokens): Inject<VerificationTokens>,
    Inject(mailer): Inject<Arc<dyn Sender>>,
    Inject(base): Inject<PublicBase>,
    Inject(limiter): Inject<RateLimiter>,
) -> Response {
    let request = NotifyRequest {
        username: body.username,
        template: body.template,
        vars: body.vars,
        dedupe_key: body.dedupe_key,
        requested: body.requested,
    };
    let context = Context {
        db: &db,
        mailer: &**mailer,
        tokens: &tokens,
        base: &base,
        limiter: &limiter,
    };

    let (status, response) = match notify::dispatch(&context, &request).await {
        Err(invalid) => {
            return json(StatusCode::BAD_REQUEST, &Problem { error: invalid.0 });
        }
        Ok(Outcome::Accepted) => (
            StatusCode::ACCEPTED,
            NotifyResponse {
                status: "accepted".into(),
                reason: None,
                retryable: None,
            },
        ),
        Ok(Outcome::Skipped(reason)) => (
            StatusCode::OK,
            NotifyResponse {
                status: "skipped".into(),
                reason: Some(reason.as_str().into()),
                retryable: None,
            },
        ),
        Ok(Outcome::Failed { message, transient }) => (
            StatusCode::BAD_GATEWAY,
            NotifyResponse {
                status: "failed".into(),
                reason: Some(message),
                retryable: Some(transient),
            },
        ),
    };
    tracing::info!(
        "notify {} for {} from {}: {}{}",
        request.template,
        request.username,
        actor.0.sub,
        response.status,
        response
            .reason
            .as_deref()
            .map(|r| format!(" ({r})"))
            .unwrap_or_default()
    );
    json(status, &response)
}

fn json<T: Serialize>(status: StatusCode, body: &T) -> Response {
    Response::json(status, body)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = notify_user as fn(_, _, _, _, _, _, _) -> _;
}
