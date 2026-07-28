//! The executor interface, and the vocabulary it speaks.
//!
//! Modelled on switchboard's `VllmEngine`: one trait, an opaque handle, and an
//! implementation per place the work can happen. The handle is a string rather
//! than an associated type so the trait stays object-safe - the scheduler holds
//! an `Arc<dyn JobExecutor>` chosen from configuration, and cannot be generic
//! over something decided at runtime.

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

/// Where a job's code comes from.
///
/// The native executor never looks at this: it runs in the checkout conveyor
/// already made. The kubernetes executor does, because its pod is somewhere
/// else entirely and has to fetch the commit for itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpec {
    pub clone_url: String,
    pub git_ref: String,
    pub sha: String,
}

/// One job, as the executor needs it.
///
/// Built from a [`crate::pipeline::Job`] plus the run's context - the timeout
/// resolved against the deployment default, and secrets already merged into
/// `env` by the caller, so an executor never sees the secret store.
#[derive(Clone, Debug)]
pub struct JobSpec {
    /// The `jobs` row this belongs to; also the handle the executor returns.
    pub id: String,
    /// `build/cargo`, for logs and error messages.
    pub name: String,
    pub steps: Vec<Step>,
    pub env: BTreeMap<String, String>,
    pub timeout: Duration,
    /// Honoured by the kubernetes executor; the native one has only the
    /// toolchain conveyor itself was given.
    pub image: Option<String>,
    /// Where to fetch the code, for an executor that runs off this machine.
    /// `None` means only a local checkout is available.
    pub source: Option<SourceSpec>,
    /// Strips injected secrets out of output.
    ///
    /// Applied by the executor rather than by the caller, so it covers the live
    /// stream as well as what is stored. A subscriber watching a running job
    /// sees the same redacted text the database will.
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
    /// Exit code of the first step that failed, or of the last step when they
    /// all passed.
    pub exit_code: Option<i32>,
    /// Why it ended, when the exit code does not say - a timeout, a
    /// cancellation, a step that could not be spawned.
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

/// One line of output.
///
/// `seq` is assigned by the executor and is contiguous per job, which is what
/// lets a reader ask for everything after what it already has and resume a
/// stream without gaps or repeats.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogChunk {
    pub seq: u64,
    pub stream: Stream,
    pub line: String,
    pub at: DateTime<Utc>,
}

/// Output so far, and output to come.
///
/// Both, because either alone is a race: a subscriber that only gets the live
/// channel misses whatever was written before it asked, and a snapshot alone
/// goes stale immediately.
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

    /// Begins the job and returns immediately. The work continues in the
    /// background; [`JobExecutor::poll`] says how it is getting on.
    async fn start(&self, spec: &JobSpec, workspace: &Workspace) -> Result<Handle, ExecError>;

    async fn poll(&self, handle: &Handle) -> Result<JobState, ExecError>;

    /// Output so far, plus a subscription to the rest.
    async fn logs(&self, handle: &Handle) -> Result<LogTail, ExecError>;

    /// Asks the job to stop. Returns once it has been asked, not once it has
    /// stopped - the job's final state arrives through `poll` like any other.
    async fn cancel(&self, handle: &Handle) -> Result<(), ExecError>;

    /// Drops whatever the executor was holding for a finished job. The
    /// scheduler calls this once it has persisted the outcome; without it a
    /// long-lived service accumulates every log line it has ever produced.
    async fn forget(&self, handle: &Handle) -> Result<(), ExecError>;
}
