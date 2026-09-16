//! GitHub: `X-Hub-Signature-256` in, commit statuses out.

use crate::domain::{Repo, Trigger};
use crate::providers::{
    CommitStatusReport, GitProvider, ProviderError, TriggerEvent, header, verify_sha256_signature,
};
use async_trait::async_trait;
use http::HeaderMap;
use serde::Deserialize;
use serde_json::json;

const SIGNATURE_HEADER: &str = "x-hub-signature-256";
const EVENT_HEADER: &str = "x-github-event";
const DELIVERY_HEADER: &str = "x-github-delivery";

/// A sha of all zeros: what a push event carries when a branch was deleted.
const ZERO_SHA: &str = "0000000000000000000000000000000000000000";

pub struct GitHubProvider {
    api_base: String,
    /// A token with `repo:status`; without it conveyor still builds, just can't report.
    token: Option<String>,
    http: reqwest::Client,
}

impl GitHubProvider {
    pub fn from_env() -> Self {
        let token = envmnt::get_or("CONVEYOR_GITHUB_TOKEN", "");
        let token = (!token.trim().is_empty()).then(|| token.trim().to_string());

        if token.is_none() {
            tracing::info!(
                "CONVEYOR_GITHUB_TOKEN is not set: conveyor will build GitHub \
                 repositories but will not report statuses back"
            );
        }

        Self {
            // Overridable for GitHub Enterprise's own host.
            api_base: envmnt::get_or("CONVEYOR_GITHUB_API", "https://api.github.com")
                .trim_end_matches('/')
                .to_string(),
            token,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .user_agent("conveyor")
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl GitProvider for GitHubProvider {
    fn name(&self) -> &'static str {
        "github"
    }

    fn verify(&self, headers: &HeaderMap, body: &[u8], secret: &[u8]) -> bool {
        let Ok(signature) = header(headers, SIGNATURE_HEADER) else {
            return false;
        };
        verify_sha256_signature(signature, body, secret)
    }

    fn parse(
        &self,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Result<Option<TriggerEvent>, ProviderError> {
        let event = header(headers, EVENT_HEADER)?;
        let delivery = header(headers, DELIVERY_HEADER)?.to_string();

        match event {
            "push" => parse_push(body, delivery),
            "pull_request" => parse_pull_request(body, delivery),
            // `ping` (hook creation) and everything else conveyor has no use for; not an error.
            other => {
                tracing::debug!("ignoring GitHub event {other}");
                Ok(None)
            }
        }
    }

    async fn report_status(
        &self,
        repo: &Repo,
        sha: &str,
        report: &CommitStatusReport,
    ) -> Result<(), ProviderError> {
        let Some(token) = &self.token else {
            return Err(ProviderError::NotConfigured("github"));
        };

        let url = format!(
            "{}/repos/{}/{}/statuses/{sha}",
            self.api_base, repo.owner, repo.name
        );

        let mut payload = json!({
            "state": report.state.as_str(),
            // GitHub errors past 140 chars, which would log a passing build as a report failure.
            "description": truncate(&report.description, 140),
            "context": report.context,
        });
        if let Some(target) = &report.target_url {
            payload["target_url"] = json!(target);
        }

        let response = self
            .http
            .post(&url)
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&payload)
            .send()
            .await
            .map_err(|source| ProviderError::Unreachable {
                provider: "github",
                source,
            })?;

        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        Err(ProviderError::Rejected {
            provider: "github",
            status: status.as_u16(),
            body: response.text().await.unwrap_or_default(),
        })
    }
}

// Events.
fn parse_push(body: &[u8], delivery: String) -> Result<Option<TriggerEvent>, ProviderError> {
    let push: PushEvent = decode(body)?;

    // A deleted branch has nothing to build; its `after` is all zeros.
    if push.deleted || push.after == ZERO_SHA {
        tracing::debug!("ignoring the deletion of {}", push.git_ref);
        return Ok(None);
    }

    Ok(Some(TriggerEvent {
        delivery_id: delivery,
        trigger: Trigger::Push,
        owner: push.repository.owner.login,
        name: push.repository.name,
        git_ref: push.git_ref,
        sha: push.after,
        message: push
            .head_commit
            .and_then(|commit| first_line(&commit.message)),
        // A push is always to the repository itself; only a pull request can carry code from elsewhere.
        from_fork: false,
    }))
}

fn parse_pull_request(
    body: &[u8],
    delivery: String,
) -> Result<Option<TriggerEvent>, ProviderError> {
    let event: PullRequestEvent = decode(body)?;

    // `synchronize` is a new push; the rest (labelled, assigned, closed) change nothing about the code.
    if !matches!(
        event.action.as_str(),
        "opened" | "reopened" | "synchronize" | "ready_for_review"
    ) {
        tracing::debug!("ignoring pull_request action {}", event.action);
        return Ok(None);
    }

    let base = event.repository.full_name.clone();
    let head_repo = event
        .pull_request
        .head
        .repo
        .as_ref()
        .map(|repo| repo.full_name.clone());
    // Absent head repo means deleted (also true for forks) - treat as fork, the safe direction.
    let from_fork = head_repo.as_deref() != Some(base.as_str());

    // A fork's branch doesn't exist in the base repo; GitHub publishes it as `refs/pull/N/head`.
    let git_ref = if from_fork {
        format!("refs/pull/{}/head", event.number)
    } else {
        format!("refs/heads/{}", event.pull_request.head.git_ref)
    };

    Ok(Some(TriggerEvent {
        delivery_id: delivery,
        trigger: Trigger::PullRequest,
        owner: event.repository.owner.login,
        name: event.repository.name,
        git_ref,
        sha: event.pull_request.head.sha,
        message: Some(event.pull_request.title),
        from_fork,
    }))
}

fn decode<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, ProviderError> {
    serde_json::from_slice(body).map_err(|error| ProviderError::Malformed {
        provider: "github",
        reason: error.to_string(),
    })
}

/// Only the subject line is worth carrying into a run listing.
fn first_line(message: &str) -> Option<String> {
    let line = message.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    // By characters, not bytes - slicing mid-character would produce invalid UTF-8.
    let kept: String = value.chars().take(limit.saturating_sub(1)).collect();
    format!("{kept}…")
}

// Payloads - only the fields conveyor reads, deliberately not exhaustive.
#[derive(Deserialize)]
struct PushEvent {
    #[serde(rename = "ref")]
    git_ref: String,
    after: String,
    #[serde(default)]
    deleted: bool,
    #[serde(default)]
    head_commit: Option<Commit>,
    repository: Repository,
}

#[derive(Deserialize)]
struct Commit {
    message: String,
}

#[derive(Deserialize)]
struct PullRequestEvent {
    action: String,
    number: u64,
    pull_request: PullRequest,
    repository: Repository,
}

#[derive(Deserialize)]
struct PullRequest {
    title: String,
    head: Head,
}

#[derive(Deserialize)]
struct Head {
    #[serde(rename = "ref")]
    git_ref: String,
    sha: String,
    #[serde(default)]
    repo: Option<RepositoryRef>,
}

#[derive(Deserialize)]
struct RepositoryRef {
    full_name: String,
}

#[derive(Deserialize)]
struct Repository {
    name: String,
    full_name: String,
    owner: Owner,
}

#[derive(Deserialize)]
struct Owner {
    login: String,
}
