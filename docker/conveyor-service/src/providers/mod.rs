//! Where triggers come from, and where results go back to.
//!
//! One trait, one implementation per provider, chosen from the repository's
//! `provider` column. The shapes differ enough - GitHub signs with
//! `X-Hub-Signature-256` and takes statuses on a REST endpoint, a plain webhook
//! signs with whatever you tell it to and takes nothing back - that a single
//! parameterised client would be a worse abstraction than a trait.

use crate::domain::{Provider, Repo, Status, Trigger};
use actix_web::http::header::HeaderMap;
use async_trait::async_trait;
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
    /// The provider's id for this delivery. Unique, which is what makes a
    /// redelivery a no-op rather than a second run.
    pub delivery_id: String,
    pub trigger: Trigger,
    pub owner: String,
    pub name: String,
    /// Full ref, ready to fetch from the registered clone url.
    pub git_ref: String,
    pub sha: String,
    pub message: Option<String>,
    /// Whether the code being built comes from outside the repository.
    ///
    /// A fork's pipeline is written by someone outside the estate, and under
    /// the native executor it would run with this service's privileges.
    pub from_fork: bool,
}

/// The four states GitHub's statuses API accepts, which is also as much
/// nuance as any provider offers.
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

    /// How a run's status reads to a provider.
    ///
    /// `Skipped` is a success: nothing ran, so nothing is wrong, and a pull
    /// request whose pipeline excluded every stage should not be blocked by a
    /// red mark. `Cancelled` is an error rather than a failure - the code was
    /// never shown to be broken, somebody stopped the build.
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
    /// Which check this is, so conveyor's mark does not collide with anyone
    /// else's on the same commit.
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

    /// Whether this delivery really came from the provider.
    ///
    /// Takes the raw body, never a parsed one: the signature covers the exact
    /// bytes that were sent, and a round trip through a deserialiser and back
    /// would not reproduce them.
    fn verify(&self, headers: &HeaderMap, body: &[u8], secret: &[u8]) -> bool;

    /// What this delivery asks for, or `None` for an event conveyor has no use
    /// for - a ping, a branch deletion, a pull request being labelled.
    fn parse(
        &self,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Result<Option<TriggerEvent>, ProviderError>;

    /// Reports a result back. Providers that have nowhere to put one do
    /// nothing and say so in their log.
    async fn report_status(
        &self,
        repo: &Repo,
        sha: &str,
        report: &CommitStatusReport,
    ) -> Result<(), ProviderError>;
}

/// The providers this deployment can talk to, built once at startup.
///
/// One instance each rather than one per request: each holds an HTTP client,
/// and building a client per status report would throw away every pooled
/// connection.
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

/// The estate-wide signing secret, from the environment.
///
/// What a single-tenant deployment needs, and the fallback for a repository
/// that has not been given one of its own. `None` means unverified deliveries
/// would be accepted, which is why the webhook endpoint refuses to serve when
/// there is neither this nor a per-repository secret.
pub fn webhook_secret() -> Option<String> {
    let secret = envmnt::get_or("CONVEYOR_WEBHOOK_SECRET", "");
    (!secret.trim().is_empty()).then(|| secret.trim().to_string())
}

/// The secret a particular repository's deliveries are signed with.
///
/// Its own `WEBHOOK_SECRET` if it has one, otherwise the estate's. Per
/// repository is the better arrangement: one compromised hook does not let
/// somebody forge deliveries for every other repository conveyor builds.
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
            // A repository with an unreadable secret must not silently fall
            // back to the estate's: that would accept deliveries signed with a
            // secret it was deliberately moved off.
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

/// Constant-time comparison of a `sha256=<hex>` signature against the body.
///
/// Shared because both providers sign the same way; only the header name
/// differs. `hmac`'s own verification is used rather than comparing strings,
/// so a wrong signature takes the same time to reject however wrong it is.
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
