//! A provider that reports nowhere and records everything - for the BDD suite and worker tests.

use crate::domain::{Repo, Trigger};
use crate::providers::{CommitStatusReport, GitProvider, ProviderError, TriggerEvent};
use async_trait::async_trait;
use http::HeaderMap;
use std::sync::Mutex;

/// One status report, as it was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reported {
    pub repo: String,
    pub sha: String,
    pub state: &'static str,
    pub description: String,
}

pub struct MockProvider {
    /// What `verify` should say - set false rather than computing a deliberately wrong signature.
    accept_signatures: Mutex<bool>,
    /// What `parse` should return.
    event: Mutex<Option<TriggerEvent>>,
    reports: Mutex<Vec<Reported>>,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self {
            accept_signatures: Mutex::new(true),
            event: Mutex::new(None),
            reports: Mutex::new(Vec::new()),
        }
    }
}

impl MockProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_accept_signatures(&self, accept: bool) {
        *self.accept_signatures.lock().expect("poisoned") = accept;
    }

    /// Scripts what the next delivery will be read as.
    pub fn set_event(&self, event: Option<TriggerEvent>) {
        *self.event.lock().expect("poisoned") = event;
    }

    /// Every status reported, in order.
    pub fn reports(&self) -> Vec<Reported> {
        self.reports.lock().expect("poisoned").clone()
    }

    /// Plausible values, for a test that only cares about one field of it.
    pub fn sample_event() -> TriggerEvent {
        TriggerEvent {
            delivery_id: "delivery-1".to_string(),
            trigger: Trigger::Push,
            owner: "tests".to_string(),
            name: "thing".to_string(),
            git_ref: "refs/heads/master".to_string(),
            sha: "0123456789abcdef0123456789abcdef01234567".to_string(),
            message: Some("a commit".to_string()),
            from_fork: false,
        }
    }
}

#[async_trait]
impl GitProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn verify(&self, _headers: &HeaderMap, _body: &[u8], _secret: &[u8]) -> bool {
        *self.accept_signatures.lock().expect("poisoned")
    }

    fn parse(
        &self,
        _headers: &HeaderMap,
        _body: &[u8],
    ) -> Result<Option<TriggerEvent>, ProviderError> {
        Ok(self.event.lock().expect("poisoned").clone())
    }

    async fn report_status(
        &self,
        repo: &Repo,
        sha: &str,
        report: &CommitStatusReport,
    ) -> Result<(), ProviderError> {
        self.reports.lock().expect("poisoned").push(Reported {
            repo: repo.slug(),
            sha: sha.to_string(),
            state: report.state.as_str(),
            description: report.description.clone(),
        });
        Ok(())
    }
}
