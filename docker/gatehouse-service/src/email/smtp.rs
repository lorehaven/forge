//! Real delivery: submit to an SMTP server (in this estate, the in-cluster
//! Stalwart) through `quench-mail`.

use super::templates::{Kind, render};
use super::{Recipient, SendError, Sender};
use async_trait::async_trait;
use quench_mail::{Address, Mailbox, Mailer, Message, Security};
use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    /// TLS from the first byte (port 465).
    Implicit,
    /// Upgrade with STARTTLS (port 587).
    StartTls,
    /// No encryption - only for a trusted local hop, and never with a login.
    None,
}

/// What `SMTP_*` / `MAIL_*` say. `None` from [`SmtpConfig::from_lookup`] means
/// mail is not configured, which is a valid (log-only) setup.
#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: Option<u16>,
    pub tls: TlsMode,
    /// Certificate name to validate, when it is not `host`.
    pub tls_server_name: Option<String>,
    pub credentials: Option<(String, String)>,
    pub from: Mailbox,
    pub timeout: Duration,
}

impl fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("tls", &self.tls)
            .field("tls_server_name", &self.tls_server_name)
            .field("username", &self.credentials.as_ref().map(|(u, _)| u))
            .field("from", &self.from.to_string())
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl SmtpConfig {
    pub fn from_env() -> Result<Option<Self>, String> {
        Self::from_lookup(&|key| std::env::var(key).ok())
    }

    /// `get` returns a variable's value; blank counts as unset.
    pub fn from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
        let var = |key: &str| {
            get(key)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };

        let Some(host) = var("SMTP_HOST") else {
            return Ok(None);
        };
        let tls = match var("SMTP_TLS")
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            None | Some("implicit") => TlsMode::Implicit,
            Some("starttls") => TlsMode::StartTls,
            Some("none") => TlsMode::None,
            Some(other) => {
                return Err(format!(
                    "SMTP_TLS must be implicit, starttls or none, not {other:?}"
                ));
            }
        };
        let port = var("SMTP_PORT")
            .map(|p| {
                p.parse::<u16>()
                    .map_err(|_| format!("SMTP_PORT {p:?} is not a port"))
            })
            .transpose()?;
        let credentials = match (var("SMTP_USERNAME"), var("SMTP_PASSWORD")) {
            (Some(user), Some(pass)) => Some((user, pass)),
            (None, None) => None,
            _ => return Err("SMTP_USERNAME and SMTP_PASSWORD must be set together".into()),
        };
        let address = var("MAIL_FROM").ok_or("MAIL_FROM is required when SMTP_HOST is set")?;
        let address: Address = address
            .parse()
            .map_err(|_| format!("MAIL_FROM {address:?} is not a valid email address"))?;
        let name = match get("MAIL_FROM_NAME") {
            Some(name) => Some(name.trim().to_string()).filter(|n| !n.is_empty()),
            None => Some("Forge".to_string()),
        };
        let timeout_secs = var("SMTP_TIMEOUT_SECS")
            .map(|t| {
                t.parse::<u64>()
                    .ok()
                    .filter(|s| *s > 0)
                    .ok_or_else(|| format!("SMTP_TIMEOUT_SECS {t:?} is not a positive number"))
            })
            .transpose()?
            .unwrap_or(15);

        Ok(Some(Self {
            host,
            port,
            tls,
            tls_server_name: var("SMTP_TLS_SERVER_NAME"),
            credentials,
            from: Mailbox::new(name, address),
            timeout: Duration::from_secs(timeout_secs),
        }))
    }
}

pub struct SmtpSender {
    mailer: Mailer,
    from: Mailbox,
    /// Hard cap on one whole delivery: the mailer's timeout applies per step,
    /// and a person is waiting on the redirect.
    overall: Duration,
}

impl SmtpSender {
    pub fn new(config: SmtpConfig) -> Result<Self, String> {
        let mut builder = Mailer::builder(&config.host)
            .security(match config.tls {
                TlsMode::Implicit => Security::Tls,
                TlsMode::StartTls => Security::StartTls,
                TlsMode::None => Security::None,
            })
            .hello_name(config.from.address.domain())
            .timeout(config.timeout);
        if let Some(port) = config.port {
            builder = builder.port(port);
        }
        if let Some(name) = &config.tls_server_name {
            builder = builder.tls_server_name(name);
        }
        if let Some((user, pass)) = &config.credentials {
            builder = builder.credentials(user, pass);
        }
        let mailer = builder.build().map_err(|err| err.to_string())?;
        Ok(Self {
            mailer,
            from: config.from,
            overall: config.timeout * 3,
        })
    }

    /// Logs whether the server accepts our login, without sending anything.
    /// A problem here must not stop gatehouse starting - the mail server may
    /// simply come up after it.
    pub fn spawn_startup_check(&self) {
        let mailer = self.mailer.clone();
        tokio::spawn(async move {
            match mailer.test_connection().await {
                Ok(()) => tracing::info!("mail: SMTP server reachable and login accepted"),
                Err(err) => tracing::warn!(
                    "mail: SMTP check failed ({err}); verification and reset emails will fail until it works"
                ),
            }
        });
    }

    async fn deliver(&self, kind: Kind, to: &Recipient<'_>, link: &str) -> Result<(), SendError> {
        let content = render(kind, to.locale, to.username, link);
        let message = Message::builder()
            .from_mailbox(self.from.clone())
            .to(to.address)
            .subject(&content.subject)
            .text(&content.text)
            .html(&content.html)
            .header("Auto-Submitted", "auto-generated")
            .build()
            .map_err(|err| SendError::permanent(format!("could not build the message: {err}")))?;

        match tokio::time::timeout(self.overall, self.mailer.send(&message)).await {
            Ok(Ok(response)) => {
                tracing::info!(
                    "email({}) accepted for {}: {}",
                    kind.label(),
                    to.address,
                    response.message()
                );
                Ok(())
            }
            Ok(Err(err)) => Err(SendError::new(err.to_string(), err.is_transient())),
            Err(_) => Err(SendError::transient("timed out sending the message")),
        }
    }
}

#[async_trait]
impl Sender for SmtpSender {
    async fn send_verification(&self, to: &Recipient<'_>, link: &str) -> Result<(), SendError> {
        self.deliver(Kind::Verification, to, link).await
    }

    async fn send_password_reset(&self, to: &Recipient<'_>, link: &str) -> Result<(), SendError> {
        self.deliver(Kind::PasswordReset, to, link).await
    }
}
