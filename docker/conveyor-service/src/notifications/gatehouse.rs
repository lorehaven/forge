//! Handing a message to gatehouse, which owns the wording, the language, the address and whether
//! the person still wants it. Conveyor only says what happened and to whom.

use super::outbox::Pending;
use async_trait::async_trait;
use quench_client::ClientCredentialsClient;
use serde_json::json;

/// What one delivery attempt came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Gatehouse took it - sent, or deliberately skipped (no address, opted out, duplicate).
    Done,
    /// Worth trying again later.
    Retry(String),
    /// Gatehouse refused it as malformed, or failed in a way retrying cannot fix.
    Rejected(String),
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn deliver(&self, message: &Pending) -> Delivery;
}

/// `POST /api/v1/notify` on gatehouse with conveyor's own machine identity.
pub struct GatehouseNotifier {
    client: ClientCredentialsClient,
}

impl GatehouseNotifier {
    /// `None` when gatehouse's URL or conveyor's secret for it is not set - notifications are then off.
    pub fn from_env() -> Option<Self> {
        let base = envmnt::get_or("GATEHOUSE_URL", "");
        let secret = envmnt::get_or("CLIENT_SECRET_CONVEYOR_GATEHOUSE", "");
        if base.trim().is_empty() || secret.trim().is_empty() {
            return None;
        }
        let base = base.trim().trim_end_matches('/');
        let tls_verify = envmnt::get_or("GATEHOUSE_TLS_VERIFY", "true")
            .parse::<bool>()
            .unwrap_or(true);

        match ClientCredentialsClient::builder(base)
            .token_url(&format!("{base}/api/v1/token"))
            .client_id("conveyor-gatehouse")
            .client_secret(secret.trim())
            .tls_verify(tls_verify)
            .build()
        {
            Ok(client) => Some(Self { client }),
            Err(error) => {
                tracing::error!("could not build the gatehouse client: {error}");
                None
            }
        }
    }
}

#[async_trait]
impl Notifier for GatehouseNotifier {
    async fn deliver(&self, message: &Pending) -> Delivery {
        // The run id as the dedupe key: gatehouse sends one message per run, kind and person even
        // if a retry crosses an attempt that had actually gone through.
        let body = json!({
            "username": message.username,
            "template": message.template,
            "vars": message.vars,
            "dedupe_key": message.run_id,
            // The person subscribed here, which stands in for gatehouse's own default for the kind.
            "requested": true,
        });

        match self
            .client
            .post::<_, serde_json::Value>("/api/v1/notify", &body)
            .await
        {
            Ok(_) => Delivery::Done,
            Err(error) => classify(&error.to_string()),
        }
    }
}

/// Reads gatehouse's refusal (`HTTP <status>: <body>`) for whether asking again could ever work.
pub fn classify(error: &str) -> Delivery {
    let permanent = error.contains("HTTP 400")
        || (error.contains("HTTP 502") && error.contains("\"retryable\":false"));
    if permanent {
        Delivery::Rejected(error.to_string())
    } else {
        Delivery::Retry(error.to_string())
    }
}
