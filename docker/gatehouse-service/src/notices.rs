//! Telling someone that something happened to their account.
//!
//! These go to the address on file, but only once it has been confirmed: an
//! unconfirmed address may have been typed by somebody who does not own it, and
//! a security notice is exactly the message that must not reach a stranger.
//! A notice that cannot be sent is logged and otherwise ignored - it never
//! turns a successful password change into a failed request.

use crate::email::{Mail, Recipient, Sender};
use crate::realm;
use quench_auth::domain::auth::User;
use quench_db::prelude::Db;

/// Where a notice may go: the address, if it is confirmed.
pub fn notice_address(user: &User) -> Option<&str> {
    let address = user
        .email
        .as_deref()
        .map(str::trim)
        .filter(|address| !address.is_empty())?;
    user.email_verified_at.is_some().then_some(address)
}

/// Sends `mail` to `user`'s confirmed address, in their saved language, else
/// `fallback_locale` (the browser's), else English. Nothing happens for a user
/// with no confirmed address.
pub async fn notify(
    mailer: &dyn Sender,
    user: &User,
    mail: &Mail<'_>,
    fallback_locale: Option<&str>,
) {
    let Some(address) = notice_address(user) else {
        return;
    };
    let recipient = Recipient {
        address,
        username: &user.username,
        locale: user.preferred_locale.as_deref().or(fallback_locale),
    };
    if let Err(err) = mailer.send(&recipient, mail).await {
        tracing::error!(
            "failed to send the {} notice for {}: {err}",
            mail.kind().label(),
            user.username
        );
    }
}

/// [`notify`] for a caller that only has the username - looks the account up.
pub async fn notify_username(
    mailer: &dyn Sender,
    db: &Db,
    username: &str,
    mail: &Mail<'_>,
    fallback_locale: Option<&str>,
) {
    match realm::get(db, username).await {
        Ok(user) => notify(mailer, &user, mail, fallback_locale).await,
        Err(err) => tracing::warn!(
            "no {} notice for {username}: could not read the account ({err:?})",
            mail.kind().label()
        ),
    }
}
