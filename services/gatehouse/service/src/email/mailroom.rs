//! Delivery through Mailroom, the estate's mail service: gatehouse renders the message and Mailroom queues, sends and retries
//! it. Gatehouse proves who it is with a `client_credentials` token it asks of itself.

use super::{Mail, Recipient, SendError, Sender, render_mail};
use mailroom_client::{Client, Config, SendRequest};
use quench_core::async_trait;
use std::time::Duration;

/// Longest a person waits for Mailroom to take a message (it only has to queue it).
const OVERALL: Duration = Duration::from_secs(20);

#[derive(Debug)]
pub struct MailroomSender {
    client: Client,
}

impl MailroomSender {
    /// # Errors
    ///
    /// A description of what is missing from the configuration.
    pub fn new(config: &Config) -> Result<Self, String> {
        Client::new(config)
            .map(|client| Self { client })
            .map_err(|e| e.message)
    }

    /// The request Mailroom is sent for a message (separate so tests can look at it).
    ///
    /// # Errors
    ///
    /// A permanent [`SendError`] for an unknown notification template.
    pub fn request(mail: &Mail<'_>, to: &Recipient<'_>) -> Result<SendRequest, SendError> {
        let content = render_mail(mail, to)?;
        // Names the kind of message, for whoever reads the queue or the headers (and for the integration suite).
        let mut headers = vec![("X-Forge-Mail".to_string(), mail.kind().label().to_string())];
        // Anything that is not part of running your own account must be easy to stop, and mail providers expect the
        // standard one-click header (RFC 8058).
        if let Mail::Notification { unsubscribe, .. } = mail {
            headers.push(("List-Unsubscribe".to_string(), format!("<{unsubscribe}>")));
            headers.push((
                "List-Unsubscribe-Post".to_string(),
                "List-Unsubscribe=One-Click".to_string(),
            ));
        }
        Ok(SendRequest {
            from: String::new(),
            to: vec![to.address.to_string()],
            subject: content.subject,
            text: content.text,
            html: Some(content.html),
            reply_to: None,
            headers,
            idempotency_key: None,
        })
    }
}

#[async_trait]
impl Sender for MailroomSender {
    async fn send(&self, to: &Recipient<'_>, mail: &Mail<'_>) -> Result<(), SendError> {
        let request = Self::request(mail, to)?;
        match tokio::time::timeout(OVERALL, self.client.send(&request)).await {
            Ok(Ok(queued)) => {
                quench_log::info!(
                    "email({}) queued for {} as {} ({})",
                    mail.kind().label(),
                    to.address,
                    queued.id,
                    queued.status
                );
                Ok(())
            }
            Ok(Err(failure)) => Err(SendError::new(failure.message, failure.transient)),
            Err(_) => Err(SendError::transient(
                "timed out handing the message to Mailroom",
            )),
        }
    }
}
