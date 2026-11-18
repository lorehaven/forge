//! How a forge service sends mail.
//!
//! No service holds SMTP credentials or an address book: it asks Mailroom to send, with a machine token Gatehouse issued for
//! it (`client_credentials`, audience `mailroom`). Mailroom queues the message, retries it, and owns the sender address and
//! the budgets. One call:
//!
//! ```no_run
//! # async fn demo() -> Result<(), mailroom_client::Failure> {
//! use mailroom_client::{Client, Config, SendRequest};
//! let client = Client::new(&Config::from_env().expect("MAILROOM_URL is set"))?;
//! client
//!     .send(&SendRequest {
//!         to: vec!["ann@example.org".into()],
//!         subject: "Build finished".into(),
//!         text: "It passed.".into(),
//!         idempotency_key: Some("run-42".into()),
//!         ..SendRequest::default()
//!     })
//!     .await?;
//! # Ok(())
//! # }
//! ```

#![allow(clippy::doc_markdown, clippy::too_long_first_doc_paragraph)]

pub use mailroom_core::{Queued, SendRequest};
use quench_client::ClientCredentialsClient;
use std::fmt;

/// Where Mailroom is and who is calling.
#[derive(Clone, PartialEq, Eq)]
pub struct Config {
    /// Mailroom's address with its path prefix, e.g. `http://mailroom.forge.svc/mailroom`.
    pub base_url: String,
    /// Gatehouse's address, e.g. `http://gatehouse.forge.svc/gatehouse`.
    pub gatehouse_url: String,
    /// This service's machine client in Gatehouse (`gatehouse-mailroom`, `conveyor-mailroom`, ...).
    pub client_id: String,
    pub client_secret: String,
    /// Verify certificates (always, outside a development estate).
    pub tls_verify: bool,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("base_url", &self.base_url)
            .field("gatehouse_url", &self.gatehouse_url)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("tls_verify", &self.tls_verify)
            .finish()
    }
}

impl Config {
    /// From the environment: `MAILROOM_URL` (unset means "Mailroom is not used": `None`), `GATEHOUSE_URL`,
    /// `MAILROOM_CLIENT_ID` (default `<SERVICE_NAME>-mailroom`), `MAILROOM_CLIENT_SECRET`, `MAILROOM_TLS_VERIFY` (default on).
    #[must_use]
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(&|key| std::env::var(key).ok())
    }

    /// The same, reading variables through `get` (blank counts as unset).
    #[must_use]
    pub fn from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        let var = |key: &str| {
            get(key)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let base_url = var("MAILROOM_URL")?;
        let service = var("SERVICE_NAME").unwrap_or_else(|| "forge".into());
        Some(Self {
            base_url,
            gatehouse_url: var("GATEHOUSE_URL").unwrap_or_default(),
            client_id: var("MAILROOM_CLIENT_ID").unwrap_or_else(|| format!("{service}-mailroom")),
            client_secret: var("MAILROOM_CLIENT_SECRET").unwrap_or_default(),
            tls_verify: var("MAILROOM_TLS_VERIFY")
                .is_none_or(|v| !matches!(v.to_ascii_lowercase().as_str(), "false" | "0" | "no")),
        })
    }
}

/// Why a send did not go through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// What went wrong, for a log.
    pub message: String,
    /// Whether trying again later could work (Mailroom busy or down, over its limit) or not (a refused request).
    pub transient: bool,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

/// A client for Mailroom's send endpoint. Cheap to clone; keeps its token until it is about to expire.
#[derive(Clone)]
pub struct Client {
    http: ClientCredentialsClient,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("mailroom_client::Client { .. }")
    }
}

impl Client {
    /// # Errors
    ///
    /// A non-transient [`Failure`] when the configuration is incomplete.
    pub fn new(config: &Config) -> Result<Self, Failure> {
        let missing = [
            ("MAILROOM_URL", &config.base_url),
            ("GATEHOUSE_URL", &config.gatehouse_url),
            ("MAILROOM_CLIENT_SECRET", &config.client_secret),
        ]
        .into_iter()
        .find(|(_, value)| value.is_empty());
        if let Some((name, _)) = missing {
            return Err(Failure {
                message: format!("{name} is not set"),
                transient: false,
            });
        }
        let token_url = format!(
            "{}/api/v1/token",
            config.gatehouse_url.trim_end_matches('/')
        );
        let http = ClientCredentialsClient::builder(config.base_url.trim_end_matches('/'))
            .token_url(&token_url)
            .client_id(&config.client_id)
            .client_secret(&config.client_secret)
            .tls_verify(config.tls_verify)
            .build()
            .map_err(|e| Failure {
                message: format!("could not build the HTTP client: {e}"),
                transient: false,
            })?;
        Ok(Self { http })
    }

    /// Asks Mailroom to send a message. Returns once it is queued.
    ///
    /// # Errors
    ///
    /// A [`Failure`], transient when Mailroom or Gatehouse could not be reached or is over its limits.
    pub async fn send(&self, request: &SendRequest) -> Result<Queued, Failure> {
        self.http
            .post::<SendRequest, Queued>("/api/v1/send", request)
            .await
            .map_err(|e| classify(&e.to_string()))
    }
}

/// Reads the HTTP status out of the client's `HTTP 429 Too Many Requests: ...` message (or a token refusal), and decides
/// whether a later retry could help. Anything that is not a clear 4xx is treated as transient: the network is the likelier cause.
#[must_use]
pub fn classify(message: &str) -> Failure {
    let status: Option<u16> = message
        .strip_prefix("HTTP ")
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|code| code.parse().ok());
    // No status: the token endpoint refusing the credentials will not get better by itself; anything else is the network.
    let transient = status.map_or_else(
        || !message.contains("grant was refused"),
        |code| code >= 500 || code == 429 || code == 408,
    );
    let message: String = message
        .chars()
        .filter(|c| !c.is_control())
        .take(300)
        .collect();
    Failure { message, transient }
}
