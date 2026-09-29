//! The link that ends one kind of notification.
//!
//! Every notification carries one. The token is not spent when used: turning a
//! subscription off is idempotent, mail providers retry the one-click POST, and
//! a link in an old email should keep working until it expires.

use crate::PublicBase;
use crate::tokens::{PURPOSE_UNSUBSCRIBE, VerificationTokens};
use crate::ui::common::ui_path;
use serde::{Deserialize, Serialize};

/// Long enough to outlast a mailbox's memory of the message.
pub const UNSUBSCRIBE_TTL_SECS: u64 = 90 * 24 * 60 * 60;

/// What an unsubscribe link stands for: this person, this kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unsubscribe {
    pub username: String,
    pub template: String,
}

impl Unsubscribe {
    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("serialises")
    }

    pub fn decode(raw: &str) -> Option<Self> {
        serde_json::from_str(raw).ok()
    }
}

/// Mints the token and returns the link for the email.
pub async fn issue_link(
    tokens: &VerificationTokens,
    base: &PublicBase,
    username: &str,
    template: &str,
) -> anyhow::Result<String> {
    let ticket = Unsubscribe {
        username: username.to_string(),
        template: template.to_string(),
    };
    let token = tokens
        .issue(PURPOSE_UNSUBSCRIBE, &ticket.encode(), UNSUBSCRIBE_TTL_SECS)
        .await?;
    Ok(format!(
        "{}{}",
        base.as_str(),
        ui_path(&format!("/unsubscribe?token={token}"))
    ))
}
