//! Delivery through Mailroom, the estate's own mail: gatehouse renders the message and Mailroom puts it into the person's mailbox
//! (`<user name>@<domain>`; the address stored in the account is not used, nothing is sent outside the estate). Gatehouse proves
//! who it is with a `client_credentials` token it asks of itself.
//!
//! A mailbox can only be read by someone who is signed in, so the messages a person needs *before* they can sign in (the
//! verification, the password reset and the invitation) could never be read there; and a reset link in the account's own
//! mailbox would let whoever holds a live session of that account set a new password. So the four messages that carry a
//! link (those three and the email change) are **not** delivered to Mailroom at all: they are written to the log as the
//! logging sender writes them (`visit <link> to ...`) and the administrator hands the link over. Everything else (notices,
//! notifications) goes to the mailbox, and also to the log while `GATEHOUSE_LOG_ACCOUNT_LINKS` is on.

use super::{LoggingSender, Mail, Recipient, SendError, Sender, render_mail};
use mailroom_client::{Client, Config, SendRequest};
use quench_core::async_trait;
use std::time::Duration;

/// Whether a message is one of the four that carry a link which grants something (an account, a password, an address).
const fn carries_a_link(mail: &Mail<'_>) -> bool {
    matches!(
        mail,
        Mail::Verification { .. }
            | Mail::PasswordReset { .. }
            | Mail::Invite { .. }
            | Mail::EmailChange { .. }
    )
}

/// Longest a person waits for Mailroom to take a message (it only has to queue it).
const OVERALL: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub struct MailroomSender {
    client: Client,
    log_links: bool,
}

impl MailroomSender {
    /// # Errors
    ///
    /// A description of what is missing from the configuration.
    pub fn new(config: &Config) -> Result<Self, String> {
        Client::new(config)
            .map(|client| Self {
                client,
                log_links: quench_core::env::is_or("GATEHOUSE_LOG_ACCOUNT_LINKS", true),
            })
            .map_err(|e| e.message)
    }

    /// Whether the account links are also written to the log.
    #[must_use]
    pub const fn with_logged_links(mut self, on: bool) -> Self {
        self.log_links = on;
        self
    }

    /// The request Mailroom is sent for a message (separate so tests can look at it).
    ///
    /// # Errors
    ///
    /// A permanent [`SendError`] for an unknown notification template.
    pub fn request(mail: &Mail<'_>, to: &Recipient<'_>) -> Result<SendRequest, SendError> {
        let content = render_mail(mail, to)?;
        // Anything that is not part of running your own account must be easy to stop: the reader shows this link.
        let unsubscribe = match mail {
            Mail::Notification { unsubscribe, .. } => Some((*unsubscribe).to_string()),
            _ => None,
        };
        Ok(SendRequest {
            to: vec![to.username.to_string()],
            subject: content.subject,
            text: content.text,
            html: Some(content.html),
            unsubscribe,
            // Names the kind of message, for whoever reads the delivery log (and for the integration suite).
            kind: Some(mail.kind().label().to_string()),
            idempotency_key: None,
        })
    }
}

#[async_trait]
impl Sender for MailroomSender {
    async fn send(&self, to: &Recipient<'_>, mail: &Mail<'_>) -> Result<(), SendError> {
        if carries_a_link(mail) {
            if !self.log_links {
                return Err(SendError::permanent(
                    "this message is a link for the administrator to hand over, and GATEHOUSE_LOG_ACCOUNT_LINKS is off",
                ));
            }
            return LoggingSender.send(to, mail).await;
        }
        let request = Self::request(mail, to)?;
        match tokio::time::timeout(OVERALL, self.client.send(&request)).await {
            Ok(Ok(sent)) => {
                quench_log::info!(
                    "email({}) delivered to user={} as {} ({})",
                    mail.kind().label(),
                    to.username,
                    sent.id,
                    sent.status
                );
                if self.log_links {
                    // The same lines the logging sender writes: the links the administrator may need to hand over.
                    let _ = LoggingSender.send(to, mail).await;
                }
                Ok(())
            }
            Ok(Err(failure)) => Err(SendError::new(failure.message, failure.transient)),
            Err(_) => Err(SendError::transient(
                "timed out handing the message to Mailroom",
            )),
        }
    }
}
