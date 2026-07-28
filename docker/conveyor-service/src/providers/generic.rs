//! A webhook from anything: a shared secret in, nothing back.
//!
//! For a repository on a host conveyor has no integration with. The sender
//! decides what to say and signs it the same way GitHub does, so a three-line
//! post-receive hook is enough to drive a build.
//!
//! ```bash
//! BODY='{"delivery_id":"'$(git rev-parse HEAD)'","owner":"me","name":"thing",
//!        "ref":"refs/heads/master","sha":"'$(git rev-parse HEAD)'"}'
//! SIG="sha256=$(printf '%s' "$BODY" | openssl dgst -sha256 -hmac "$SECRET" -r | cut -d' ' -f1)"
//! curl -X POST "$CONVEYOR/api/v1/webhooks/generic" \
//!      -H "X-Conveyor-Signature-256: $SIG" -d "$BODY"
//! ```

use crate::domain::{Repo, Trigger};
use crate::providers::{
    CommitStatusReport, GitProvider, ProviderError, TriggerEvent, header, verify_sha256_signature,
};
use actix_web::http::header::HeaderMap;
use async_trait::async_trait;
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
            // Nothing in a plain webhook says where the code came from, so the
            // sender is trusted - which is what the shared secret establishes.
            from_fork: false,
        }))
    }

    async fn report_status(
        &self,
        repo: &Repo,
        sha: &str,
        report: &CommitStatusReport,
    ) -> Result<(), ProviderError> {
        // Deliberately not an error: a repository registered as `generic` is
        // one conveyor has no API for, and failing every run's final step over
        // something it was never going to be able to do would be wrong.
        tracing::debug!(
            "generic provider has nowhere to report {} for {}@{sha}",
            report.state.as_str(),
            repo.slug()
        );
        Ok(())
    }
}

/// What a sender posts. Deliberately small - anything conveyor can work out for
/// itself is not asked for.
#[derive(Deserialize)]
struct GenericEvent {
    /// Anything unique per delivery. A redelivery with the same id is ignored,
    /// which is what makes a retrying sender safe.
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
