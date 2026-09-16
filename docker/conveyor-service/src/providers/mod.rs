//! Where triggers come from, and where results go back to - one trait, one implementation per
//! provider (the shapes differ too much for a single parameterised client).

use crate::domain::{Provider, Repo, Status, Trigger};
use async_trait::async_trait;
use http::HeaderMap;
use std::sync::Arc;

pub mod generic;
pub mod github;
pub mod mock;

pub use generic::GenericProvider;
pub use github::GitHubProvider;
pub use mock::MockProvider;

/// What a delivery asks conveyor to build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerEvent {
    /// Unique, so a redelivery is a no-op rather than a second run.
    pub delivery_id: String,
    pub trigger: Trigger,
    pub owner: String,
    pub name: String,
    /// Full ref, ready to fetch from the registered clone url.
    pub git_ref: String,
    pub sha: String,
    pub message: Option<String>,
    /// A fork's pipeline runs with this service's privileges under native - see `allow_fork_pr`.
    pub from_fork: bool,
}

/// The four states GitHub's statuses API accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitState {
    Pending,
    Success,
    Failure,
    Error,
}

impl CommitState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Error => "error",
        }
    }

    /// `Skipped` reads as success (nothing ran, nothing's wrong); `Cancelled` as error, not failure.
    pub const fn from_status(status: Status) -> Self {
        match status {
            Status::Queued | Status::Running => Self::Pending,
            Status::Success | Status::Skipped => Self::Success,
            Status::Failed => Self::Failure,
            Status::Cancelled => Self::Error,
        }
    }
}

/// What conveyor tells a provider about a commit.
#[derive(Clone, Debug)]
pub struct CommitStatusReport {
    pub state: CommitState,
    /// One line, shown next to the mark.
    pub description: String,
    /// Where to send someone who clicks it.
    pub target_url: Option<String>,
    /// Which check this is, so conveyor's mark doesn't collide with anyone else's.
    pub context: String,
}

impl CommitStatusReport {
    pub fn new(status: Status, description: impl Into<String>) -> Self {
        Self {
            state: CommitState::from_status(status),
            description: description.into(),
            target_url: None,
            context: envmnt::get_or("CONVEYOR_STATUS_CONTEXT", "conveyor"),
        }
    }

    #[must_use]
    pub fn with_target(mut self, url: Option<String>) -> Self {
        self.target_url = url;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("missing {header}")]
    MissingHeader { header: &'static str },

    #[error("{provider} sent an event conveyor cannot read: {reason}")]
    Malformed {
        provider: &'static str,
        reason: String,
    },

    #[error("{0} is not configured to report statuses: set CONVEYOR_GITHUB_TOKEN")]
    NotConfigured(&'static str),

    #[error("{provider} rejected the status report ({status}): {body}")]
    Rejected {
        provider: &'static str,
        status: u16,
        body: String,
    },

    #[error("could not reach {provider}: {source}")]
    Unreachable {
        provider: &'static str,
        #[source]
        source: reqwest::Error,
    },
}

#[async_trait]
pub trait GitProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Takes the raw body, not a parsed one - the signature covers exact bytes, which a deserialise round trip won't reproduce.
    fn verify(&self, headers: &HeaderMap, body: &[u8], secret: &[u8]) -> bool;

    /// `None` for an event conveyor has no use for (a ping, a branch deletion, ...).
    fn parse(
        &self,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Result<Option<TriggerEvent>, ProviderError>;

    /// Providers with nowhere to put a result do nothing and log it.
    async fn report_status(
        &self,
        repo: &Repo,
        sha: &str,
        report: &CommitStatusReport,
    ) -> Result<(), ProviderError>;
}

/// Built once at startup - one instance each, not one per request, so HTTP connections stay pooled.
pub struct Providers {
    github: Arc<GitHubProvider>,
    generic: Arc<GenericProvider>,
}

impl Providers {
    pub fn from_env() -> Self {
        Self {
            github: Arc::new(GitHubProvider::from_env()),
            generic: Arc::new(GenericProvider::new()),
        }
    }

    pub fn get(&self, provider: Provider) -> Arc<dyn GitProvider> {
        match provider {
            Provider::GitHub => self.github.clone(),
            Provider::Generic => self.generic.clone(),
        }
    }

    /// Resolves the path segment a webhook arrived on.
    pub fn by_name(&self, name: &str) -> Option<(Provider, Arc<dyn GitProvider>)> {
        let provider = Provider::parse(name)?;
        Some((provider, self.get(provider)))
    }
}

impl Default for Providers {
    fn default() -> Self {
        Self::from_env()
    }
}

/// Estate-wide fallback for a repository with no secret of its own; `None` means the webhook endpoint refuses to serve.
pub fn webhook_secret() -> Option<String> {
    let secret = envmnt::get_or("CONVEYOR_WEBHOOK_SECRET", "");
    (!secret.trim().is_empty()).then(|| secret.trim().to_string())
}

/// Its own secret if set, else the estate's - so one compromised hook can't forge deliveries estate-wide.
pub async fn webhook_secret_for(
    db: &quench_db::prelude::Db,
    key: Option<&crate::secrets::SecretKey>,
    repo: &Repo,
) -> Option<String> {
    if let Some(key) = key {
        let scope = crate::secrets::Scope::Repo(repo.id.clone());
        match crate::secrets::store::get(db, key, &scope, crate::secrets::WEBHOOK_SECRET_NAME).await
        {
            Ok(Some(secret)) => return Some(secret),
            Ok(None) => {}
            // Must not silently fall back to the estate's secret on a read error.
            Err(error) => {
                tracing::error!(
                    "could not read the webhook secret for {}: {error}",
                    repo.slug()
                );
                return None;
            }
        }
    }

    webhook_secret()
}

/// Constant-time via `hmac`'s own verify, not string comparison, so a wrong signature always takes the same time to reject.
pub(crate) fn verify_sha256_signature(signature: &str, body: &[u8], secret: &[u8]) -> bool {
    use hmac::{KeyInit, Mac, SimpleHmac};

    let Some(hex_digest) = signature.trim().strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(hex_digest) else {
        return false;
    };

    let Ok(mut mac) = SimpleHmac::<sha2::Sha256>::new_from_slice(secret) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

/// The signature conveyor would send for this body, for tests and for
/// documenting what a sender has to produce.
pub fn sign_sha256(body: &[u8], secret: &[u8]) -> String {
    use hmac::{KeyInit, Mac, SimpleHmac};

    let mut mac = SimpleHmac::<sha2::Sha256>::new_from_slice(secret)
        .expect("HMAC accepts a key of any length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Reads a header as a string, or says which one was missing.
pub(crate) fn header<'a>(
    headers: &'a HeaderMap,
    name: &'static str,
) -> Result<&'a str, ProviderError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or(ProviderError::MissingHeader { header: name })
}
