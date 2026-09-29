//! Changing your own email address, with proof the new one is yours.
//!
//! Asking sends a link to the NEW address; the change only happens when it is
//! followed (and then the old address is told). Until then nothing about the
//! account changes, so a typo - or someone else's address - costs nothing.

use crate::PublicBase;
use crate::tokens::{PURPOSE_EMAIL_CHANGE, VerificationTokens};
use crate::ui::common::ui_path;
use serde::{Deserialize, Serialize};

/// A day: the person is at their keyboard, waiting for the mail.
pub const EMAIL_CHANGE_TTL_SECS: u64 = 24 * 60 * 60;

/// What a confirmation link stands for: this account, this new address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    pub username: String,
    pub new_email: String,
}

impl Ticket {
    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("a ticket always serialises")
    }

    /// `None` for anything that is not a ticket this module wrote.
    pub fn decode(raw: &str) -> Option<Self> {
        serde_json::from_str(raw).ok()
    }
}

/// Mints the token for `ticket` and returns the link to email to the new address.
pub async fn issue_link(
    tokens: &VerificationTokens,
    base: &PublicBase,
    ticket: &Ticket,
) -> anyhow::Result<String> {
    let token = tokens
        .issue(
            PURPOSE_EMAIL_CHANGE,
            &ticket.encode(),
            EMAIL_CHANGE_TTL_SECS,
        )
        .await?;
    Ok(format!(
        "{}{}",
        base.as_str(),
        ui_path(&format!("/confirm-email?token={token}"))
    ))
}
