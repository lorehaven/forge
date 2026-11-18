//! Sending through a user's or the estate's SMTP submission server, behind the address policy.

use crate::compose::Built;
use crate::imap::Auth;
use crate::netguard::{self, Policy};
use crate::presets::Security;
use quench_mail::Mailer;
use std::time::Duration;

/// An SMTP submission server and the login for it.
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub username: String,
    pub auth: Auth,
    pub policy: Policy,
    pub timeout: Duration,
    pub tls_roots_pem: Vec<String>,
    /// The name the certificate must match, when it is not `host`.
    pub tls_server_name: Option<String>,
    /// What to say in EHLO.
    pub hello_name: String,
}

impl Config {
    /// A config with the defaults for everything but the server and the login.
    #[must_use]
    pub fn new(host: &str, port: u16, security: Security, username: &str, auth: Auth) -> Self {
        Self {
            host: host.to_string(),
            port,
            security,
            username: username.to_string(),
            auth,
            policy: Policy::default(),
            timeout: Duration::from_secs(30),
            tls_roots_pem: Vec::new(),
            tls_server_name: None,
            hello_name: "mailroom".to_string(),
        }
    }
}

/// Why a send failed. `transient` says a retry later may work (a busy server, a network blip); a refused recipient or login
/// will not get better.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendError {
    pub message: String,
    pub transient: bool,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SendError {}

fn mailer(config: &Config, address: &str) -> Result<Mailer, SendError> {
    let Auth::Password(password) = &config.auth else {
        return Err(SendError {
            message: "sending with OAuth is not supported yet".into(),
            transient: false,
        });
    };
    let security = match config.security {
        Security::Tls => quench_mail::Security::Tls,
        Security::StartTls => quench_mail::Security::StartTls,
    };
    // Dial the checked address, verify the certificate against the real name.
    let mut builder = Mailer::builder(address)
        .security(security)
        .port(config.port)
        .tls_server_name(config.tls_server_name.as_deref().unwrap_or(&config.host))
        .credentials(&config.username, password)
        .hello_name(&config.hello_name)
        .timeout(config.timeout);
    for pem in &config.tls_roots_pem {
        builder = builder
            .add_root_certificate_pem(pem)
            .map_err(|e| SendError {
                message: e.to_string(),
                transient: false,
            })?;
    }
    builder.build().map_err(|e| SendError {
        message: e.to_string(),
        transient: false,
    })
}

async fn resolve(config: &Config) -> Result<Vec<String>, SendError> {
    let addresses = netguard::resolve(&config.host, config.port, &config.policy)
        .await
        .map_err(|r| SendError {
            message: r.to_string(),
            transient: matches!(r, netguard::Refusal::NoAddress),
        })?;
    Ok(addresses.iter().map(|a| a.ip().to_string()).collect())
}

/// Runs `attempt` against each checked address in turn until one gets as far as a server's answer: a refused or silent
/// address moves on to the next, a server that answered (even to refuse) ends it.
async fn over_addresses<T, F, Fut>(config: &Config, attempt: F) -> Result<T, SendError>
where
    F: Fn(Mailer) -> Fut,
    Fut: std::future::Future<Output = Result<T, quench_mail::Error>>,
{
    let mut last = SendError {
        message: "could not connect to the mail server".into(),
        transient: true,
    };
    for address in resolve(config).await? {
        let mailer = mailer(config, &address)?;
        match attempt(mailer).await {
            Ok(value) => return Ok(value),
            Err(e @ (quench_mail::Error::Io(_) | quench_mail::Error::Timeout(_))) => {
                last = SendError {
                    transient: e.is_transient(),
                    message: e.to_string(),
                };
            }
            Err(e) => {
                return Err(SendError {
                    transient: e.is_transient(),
                    message: e.to_string(),
                });
            }
        }
    }
    Err(last)
}

/// Sends a built message to its envelope's recipients.
///
/// # Errors
///
/// A [`SendError`].
pub async fn send(config: &Config, built: &Built) -> Result<(), SendError> {
    over_addresses(config, |mailer| async move {
        mailer
            .send_raw(&built.envelope, &built.raw)
            .await
            .map(|_| ())
    })
    .await
}

/// Connects and logs in without sending anything.
///
/// # Errors
///
/// A [`SendError`].
pub async fn test(config: &Config) -> Result<(), SendError> {
    over_addresses(
        config,
        |mailer| async move { mailer.test_connection().await },
    )
    .await
}
