//! One execution of one repository's pipeline, at one commit.
//!
//! A run is also its own queue entry: `status`, `claimed_by` and `claimed_at`
//! are what the scheduler's claim loop reads and writes. Keeping the queue in
//! the same row as the record means a run cannot be queued twice, and a
//! restart loses nothing.

use crate::domain::Status;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    #[default]
    Push,
    PullRequest,
    /// Started by a person, through the UI or the CLI.
    Manual,
}

impl Trigger {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::PullRequest => "pull_request",
            Self::Manual => "manual",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "push" => Some(Self::Push),
            "pull_request" | "pullrequest" | "pr" => Some(Self::PullRequest),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

impl fmt::Display for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub repo_id: String,
    pub trigger: Trigger,
    /// Branch or tag ref that triggered this, in full form (`refs/heads/master`).
    pub git_ref: String,
    /// The commit actually built. Every later decision reads this, not the ref:
    /// the ref can move while the run is queued.
    pub sha: String,
    /// Head commit message, kept for the UI so listing runs needs no checkout.
    pub message: Option<String>,
    /// Provider delivery id, when a webhook started this. Unique, which is what
    /// makes a redelivered webhook a no-op rather than a second run.
    pub delivery_id: Option<String>,
    pub status: Status,

    pub queued_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,

    /// Worker that holds this run, and when it last said so. A claim that stops
    /// being refreshed is how a worker that died is detected.
    pub claimed_by: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub attempt: i32,

    /// Why the run ended the way it did, when that is not obvious from the jobs
    /// - a missing `.conveyor.toml`, a checkout that failed, a cycle in `needs`.
    pub error: Option<String>,

    /// The run this one restarted, if it did. Stages that passed there are
    /// carried over rather than rebuilt - see `worker::execute_jobs`.
    pub resumed_from: Option<String>,
}

impl Run {
    /// The short sha people actually read.
    pub fn short_sha(&self) -> &str {
        let end = self.sha.len().min(7);
        &self.sha[..end]
    }

    /// `master` out of `refs/heads/master`, for display and for `when` clauses.
    pub fn ref_name(&self) -> &str {
        self.git_ref
            .strip_prefix("refs/heads/")
            .or_else(|| self.git_ref.strip_prefix("refs/tags/"))
            .unwrap_or(&self.git_ref)
    }

    pub fn is_tag(&self) -> bool {
        self.git_ref.starts_with("refs/tags/")
    }
}
