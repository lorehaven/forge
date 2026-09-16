//! The executor interface. Modelled on switchboard's `VllmEngine`: one trait, opaque string handle
//! (not an associated type) so it stays object-safe for `Arc<dyn JobExecutor>`.

use crate::domain::Status;
use crate::pipeline::Step;
use crate::secrets::Redactor;
use crate::steps::StepError;
use crate::workspace::Workspace;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::broadcast;

/// Where a job's code comes from. Native ignores this (runs in conveyor's own checkout); kubernetes fetches it itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpec {
    pub clone_url: String,
    pub git_ref: String,
    pub sha: String,
    /// For kubernetes's own init-container checkout only - never given to the step container.
    pub credential: Option<JobCredential>,
}

/// Owned, not borrowed like `workspace::checkout::HttpCredential` - a `JobSpec` outlives the resolving call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobCredential {
    pub username: String,
    pub token: String,
}

/// Built from a [`crate::pipeline::Job`] plus run context; secrets are already merged into `env`.
#[derive(Clone, Debug)]
pub struct JobSpec {
    /// The `jobs` row this belongs to; also the handle the executor returns.
    pub id: String,
    /// `build/cargo`, for logs and error messages.
    pub name: String,
    pub steps: Vec<Step>,
    pub env: BTreeMap<String, String>,
    pub timeout: Duration,
    /// Honoured by kubernetes; native only has conveyor's own toolchain.
    pub image: Option<String>,
    /// `None` means only a local checkout is available.
    pub source: Option<SourceSpec>,
    /// Applied by the executor, not the caller, so it covers the live stream too.
    pub redactor: Redactor,
}

/// What the executor calls the job it started.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Handle(String);

impl Handle {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a job has got to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobState {
    pub status: Status,
    /// First failing step's exit code, or the last step's if all passed.
    pub exit_code: Option<i32>,
    /// Why it ended, when the exit code doesn't say (timeout, cancellation, spawn failure).
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    /// One entry per step in the spec, in order, always.
    pub steps: Vec<StepState>,
}

impl JobState {
    pub fn is_finished(&self) -> bool {
        self.status.is_terminal()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepState {
    pub ordinal: usize,
    pub kind: String,
    /// The command as it was run.
    pub command: String,
    pub status: Status,
    pub exit_code: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "stdout" => Some(Self::Stdout),
            "stderr" => Some(Self::Stderr),
            _ => None,
        }
    }
}

/// `seq` is contiguous per job, so a reader can resume a stream without gaps or repeats.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogChunk {
    pub seq: u64,
    pub stream: Stream,
    pub line: String,
    pub at: DateTime<Utc>,
}

/// Both halves needed: live-only misses history, snapshot-only goes stale immediately.
#[derive(Debug)]
pub struct LogTail {
    pub history: Vec<LogChunk>,
    pub live: broadcast::Receiver<LogChunk>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("no such job: {0}")]
    UnknownHandle(Handle),

    #[error("job '{job}' has no steps")]
    NoSteps { job: String },

    #[error("{0}")]
    Step(#[from] StepError),

    #[error("{executor} executor cannot {what}")]
    Unsupported {
        executor: &'static str,
        what: String,
    },

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

#[async_trait]
pub trait JobExecutor: Send + Sync {
    /// A name for logs and error messages.
    fn name(&self) -> &'static str;

    /// Returns immediately; [`JobExecutor::poll`] says how it's getting on.
    async fn start(&self, spec: &JobSpec, workspace: &Workspace) -> Result<Handle, ExecError>;

    async fn poll(&self, handle: &Handle) -> Result<JobState, ExecError>;

    /// Output so far, plus a subscription to the rest.
    async fn logs(&self, handle: &Handle) -> Result<LogTail, ExecError>;

    /// Returns once asked, not once stopped - final state arrives through `poll`.
    async fn cancel(&self, handle: &Handle) -> Result<(), ExecError>;

    /// Called once the scheduler has persisted the outcome, or logs accumulate forever.
    async fn forget(&self, handle: &Handle) -> Result<(), ExecError>;
}
