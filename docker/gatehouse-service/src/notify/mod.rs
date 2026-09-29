//! Notifications on behalf of other services.
//!
//! A service tells gatehouse "this happened to that person" and gatehouse
//! decides whether, and how, to say so: is there such a person, do they have a
//! confirmed address, did they ask for this kind of message, are they being
//! flooded. Only then does an email go out - in their language, from the one
//! sender, with a working unsubscribe.

pub mod catalog;
pub mod prefs;
pub mod unsubscribe;

pub use catalog::{Template, VarKind};
pub use prefs::Preferences;

use crate::PublicBase;
use crate::email::{Mail, Recipient, Sender};
use crate::notices::notice_address;
use crate::ratelimit::{Limit, RateLimiter, policy};
use crate::realm::{self, RealmError};
use crate::tokens::VerificationTokens;
use quench_db::prelude::Db;
use std::collections::BTreeMap;

/// One request from a service.
#[derive(Debug, Clone)]
pub struct Request {
    pub username: String,
    pub template: String,
    pub vars: BTreeMap<String, String>,
    /// Sending the same key again (a retry) does nothing for a day.
    pub dedupe_key: Option<String>,
    /// The person asked for exactly this (a subscription held by the calling
    /// service), so a template that is off by default still goes out. Their own
    /// explicit opt-out here still wins.
    pub requested: bool,
}

/// Why nothing was sent, for a caller that wants to know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    NoSuchUser,
    NoVerifiedEmail,
    NotSubscribed,
    Duplicate,
    RateLimited,
}

impl Skip {
    pub const fn as_str(self) -> &'static str {
        match self {
            Skip::NoSuchUser => "no_such_user",
            Skip::NoVerifiedEmail => "no_verified_email",
            Skip::NotSubscribed => "not_subscribed",
            Skip::Duplicate => "duplicate",
            Skip::RateLimited => "rate_limited",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Handed to the mail server.
    Accepted,
    /// Deliberately not sent.
    Skipped(Skip),
    /// Should have been sent and was not; `transient` says whether retrying can help.
    Failed { message: String, transient: bool },
}

/// The request itself is wrong - the caller's bug, not a reason to skip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid(pub String);

pub struct Context<'a> {
    pub db: &'a Db,
    pub mailer: &'a dyn Sender,
    pub tokens: &'a VerificationTokens,
    pub base: &'a PublicBase,
    pub limiter: &'a RateLimiter,
}

pub async fn dispatch(ctx: &Context<'_>, request: &Request) -> Result<Outcome, Invalid> {
    let template = catalog::find(&request.template)
        .ok_or_else(|| Invalid(format!("no such notification: {}", request.template)))?;
    template
        .validate(&request.vars, ctx.base.as_str())
        .map_err(Invalid)?;

    // The recipient: a real account with a confirmed address.
    let user = match realm::get(ctx.db, &request.username).await {
        Ok(user) => user,
        Err(RealmError::NotFound) => return Ok(Outcome::Skipped(Skip::NoSuchUser)),
        Err(err) => return Ok(failed(format!("could not read the account: {err:?}"), true)),
    };
    let Some(address) = notice_address(&user) else {
        return Ok(Outcome::Skipped(Skip::NoVerifiedEmail));
    };

    // Their choice. Asked before the limits so declining costs nothing.
    match Preferences::new(ctx.db)
        .wants(&user.username, template, request.requested)
        .await
    {
        Ok(true) => {}
        Ok(false) => return Ok(Outcome::Skipped(Skip::NotSubscribed)),
        Err(err) => return Ok(failed(format!("could not read preferences: {err}"), true)),
    }

    if let Some(key) = request.dedupe_key.as_deref().filter(|k| !k.is_empty()) {
        let subject = format!("{}|{}|{key}", template.id, user.username);
        let seen = ctx
            .limiter
            .check("notify-dedupe", &subject, Limit::per_day(1))
            .await;
        if !seen.is_allowed() {
            return Ok(Outcome::Skipped(Skip::Duplicate));
        }
    }

    let verdict = ctx
        .limiter
        .check_all(&[
            ("notify-user", &user.username, policy::NOTIFY_USER),
            (
                "notify-user-template",
                &format!("{}|{}", user.username, template.id),
                policy::NOTIFY_TEMPLATE,
            ),
        ])
        .await;
    if !verdict.is_allowed() {
        return Ok(Outcome::Skipped(Skip::RateLimited));
    }

    let unsubscribe =
        match unsubscribe::issue_link(ctx.tokens, ctx.base, &user.username, template.id).await {
            Ok(link) => link,
            Err(err) => {
                return Ok(failed(
                    format!("could not make an unsubscribe link: {err}"),
                    true,
                ));
            }
        };

    let vars: Vec<(&str, &str)> = request
        .vars
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let recipient = Recipient {
        address,
        username: &user.username,
        locale: user.preferred_locale.as_deref(),
    };
    let mail = Mail::Notification {
        template: template.id,
        vars: &vars,
        unsubscribe: &unsubscribe,
    };
    Ok(match ctx.mailer.send(&recipient, &mail).await {
        Ok(()) => Outcome::Accepted,
        Err(err) => failed(err.to_string(), err.is_transient()),
    })
}

fn failed(message: String, transient: bool) -> Outcome {
    tracing::error!("notification not sent: {message}");
    Outcome::Failed { message, transient }
}
