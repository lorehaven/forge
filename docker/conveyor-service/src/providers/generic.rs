//! A webhook from anything: a shared secret in, nothing back - for a host conveyor has no integration with.

use crate::domain::{Repo, Trigger};
use crate::providers::{
    CommitStatusReport, GitProvider, ProviderError, TriggerEvent, header, verify_sha256_signature,
};
use async_trait::async_trait;
use http::HeaderMap;
use serde::Deserialize;

const SIGNATURE_HEADER: &str = "x-conveyor-signature-256";

#[derive(Default)]
pub struct GenericProvider;

impl GenericProvider {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl GitProvider for GenericProvider {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn verify(&self, headers: &HeaderMap, body: &[u8], secret: &[u8]) -> bool {
        let Ok(signature) = header(headers, SIGNATURE_HEADER) else {
            return false;
        };
        verify_sha256_signature(signature, body, secret)
    }

    fn parse(
        &self,
        _headers: &HeaderMap,
        body: &[u8],
    ) -> Result<Option<TriggerEvent>, ProviderError> {
        let payload: GenericEvent =
            serde_json::from_slice(body).map_err(|error| ProviderError::Malformed {
                provider: "generic",
                reason: error.to_string(),
            })?;

        let trigger = match payload.event.as_deref() {
            None | Some("push") => Trigger::Push,
            Some("pull_request") => Trigger::PullRequest,
            Some(other) => {
                return Err(ProviderError::Malformed {
                    provider: "generic",
                    reason: format!("unknown event '{other}'"),
                });
            }
        };

        Ok(Some(TriggerEvent {
            delivery_id: payload.delivery_id,
            trigger,
            owner: payload.owner,
            name: payload.name,
            git_ref: payload.git_ref,
            sha: payload.sha,
            message: payload.message,
            // Nothing says where the code came from; the shared secret is what establishes trust.
            from_fork: false,
        }))
    }

    async fn report_status(
        &self,
        repo: &Repo,
        sha: &str,
        report: &CommitStatusReport,
    ) -> Result<(), ProviderError> {
        // Not an error - a `generic` repo has no API conveyor can report to.
        tracing::debug!(
            "generic provider has nowhere to report {} for {}@{sha}",
            report.state.as_str(),
            repo.slug()
        );
        Ok(())
    }
}

/// What a sender posts - deliberately small, nothing conveyor can work out itself.
#[derive(Deserialize)]
struct GenericEvent {
    /// Unique per delivery; a repeat with the same id is ignored, making a retrying sender safe.
    delivery_id: String,
    owner: String,
    name: String,
    #[serde(rename = "ref")]
    git_ref: String,
    sha: String,
    #[serde(default)]
    message: Option<String>,
    /// `push` (the default) or `pull_request`.
    #[serde(default)]
    event: Option<String>,
}
