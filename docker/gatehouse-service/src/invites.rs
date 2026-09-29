//! Inviting someone to an account an administrator created for them.
//!
//! The account starts with a password nobody knows and an address nobody has
//! confirmed. The emailed link lets the person choose their own password, and
//! following it also confirms the address - so the administrator never handles
//! the final password, and the address is proven to reach a real person.

use crate::PublicBase;
use crate::email::{Mail, Recipient, Sender};
use crate::tokens::{PURPOSE_INVITE, VerificationTokens};
use crate::ui::common::ui_path;
use quench_auth::domain::auth::User;

/// An invitation is good for a week: it is sent by a person to a person, who may
/// not read their mail the same day.
pub const INVITE_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// A password nobody is told: a fresh random one for an account that is invited
/// rather than given one, so the account is unusable until the invitation is
/// accepted.
pub fn unusable_password() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Mints an invitation token for `user` and emails it to the address on file, in
/// their saved language, else `fallback_locale`, else English. `Err` carries why
/// it could not be sent - the account exists either way, so the caller reports
/// it rather than failing.
pub async fn send_invite(
    tokens: &VerificationTokens,
    mailer: &dyn Sender,
    base: &PublicBase,
    user: &User,
    fallback_locale: Option<&str>,
) -> Result<(), String> {
    let address = user
        .email
        .as_deref()
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .ok_or("the account has no email address")?;
    let token = tokens
        .issue(PURPOSE_INVITE, &user.username, INVITE_TTL_SECS)
        .await
        .map_err(|err| format!("could not issue an invitation token: {err}"))?;
    let link = format!(
        "{}{}",
        base.as_str(),
        ui_path(&format!("/accept-invite?token={token}"))
    );
    let recipient = Recipient {
        address,
        username: &user.username,
        locale: user.preferred_locale.as_deref().or(fallback_locale),
    };
    mailer
        .send(&recipient, &Mail::Invite { link: &link })
        .await
        .map_err(|err| err.to_string())
}
