//! An executor that runs nothing and says what it was told to say.
//!
//! For the BDD suite and for tests of the scheduler, which needs a job that
//! fails on demand and finishes instantly - neither of which a real build
//! gives you reliably. It also records what it was asked to run, so a test can
//! assert that a stage was skipped by checking that no job from it ever
//! started, rather than by inspecting the plan a second time.

use crate::domain::Status;
use crate::executors::engine::{
    ExecError, Handle, JobExecutor, JobSpec, JobState, LogChunk, LogTail, StepState, Stream,
};
use crate::workspace::Workspace;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// What the mock should do when a job with a given name starts.
#[derive(Clone, Debug)]
pub struct MockOutcome {
    pub status: Status,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    /// Emitted on stdout, in order, before the job finishes.
    pub lines: Vec<String>,
}

impl Default for MockOutcome {
    fn default() -> Self {
        Self {
            status: Status::Success,
            exit_code: Some(0),
            error: None,
            lines: Vec::new(),
        }
    }
}

impl MockOutcome {
    pub fn success() -> Self {
        Self::default()
    }

    pub fn failure(exit_code: i32) -> Self {
        Self {
            status: Status::Failed,
            exit_code: Some(exit_code),
            error: None,
            lines: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_lines(mut self, lines: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.lines = lines.into_iter().map(Into::into).collect();
        self
    }
}

struct Finished {
    state: JobState,
    history: Vec<LogChunk>,
    publisher: broadcast::Sender<LogChunk>,
}

#[derive(Default)]
pub struct MockExecutor {
    /// Keyed by job name, so a test scripts "the deploy job fails" without
    /// knowing the id the scheduler will generate.
    outcomes: Mutex<HashMap<String, MockOutcome>>,
    default_outcome: Mutex<MockOutcome>,
    started: Mutex<Vec<JobSpec>>,
    cancelled: Mutex<Vec<Handle>>,
    jobs: Arc<Mutex<HashMap<String, Finished>>>,
}

impl MockExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Scripts the outcome for every job with this name.
    pub fn set_outcome(&self, job_name: impl Into<String>, outcome: MockOutcome) {
        self.outcomes
            .lock()
            .expect("outcomes poisoned")
            .insert(job_name.into(), outcome);
    }

    /// Scripts the outcome for jobs with no outcome of their own.
    pub fn set_default_outcome(&self, outcome: MockOutcome) {
        *self.default_outcome.lock().expect("outcomes poisoned") = outcome;
    }

    /// Every job that was started, in order.
    pub fn started(&self) -> Vec<JobSpec> {
        self.started.lock().expect("started poisoned").clone()
    }

    /// The names of every job that was started, in order.
    pub fn started_names(&self) -> Vec<String> {
        self.started().into_iter().map(|spec| spec.name).collect()
    }

    pub fn cancelled(&self) -> Vec<Handle> {
        self.cancelled.lock().expect("cancelled poisoned").clone()
    }

    fn outcome_for(&self, name: &str) -> MockOutcome {
        self.outcomes
            .lock()
            .expect("outcomes poisoned")
            .get(name)
            .cloned()
            .unwrap_or_else(|| {
                self.default_outcome
                    .lock()
                    .expect("outcomes poisoned")
                    .clone()
            })
    }
}

#[async_trait]
impl JobExecutor for MockExecutor {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn start(&self, spec: &JobSpec, _workspace: &Workspace) -> Result<Handle, ExecError> {
        if spec.steps.is_empty() {
            return Err(ExecError::NoSteps {
                job: spec.name.clone(),
            });
        }

        self.started
            .lock()
            .expect("started poisoned")
            .push(spec.clone());

        let outcome = self.outcome_for(&spec.name);
        let now = Utc::now();

        let history: Vec<LogChunk> = outcome
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| LogChunk {
                seq: index as u64,
                stream: Stream::Stdout,
                // Redacted here as well, so a test written against the mock
                // asserts the same behaviour the native executor has.
                line: spec.redactor.apply(line),
                at: now,
            })
            .collect();

        // Every step takes the job's outcome. The mock exists to control what a
        // job did, not to simulate a partial failure through a step list.
        let steps = spec
            .steps
            .iter()
            .enumerate()
            .map(|(ordinal, step)| StepState {
                ordinal,
                kind: step.kind().to_string(),
                command: step.command().to_string(),
                status: outcome.status,
                exit_code: outcome.exit_code,
                started_at: Some(now),
                finished_at: Some(now),
            })
            .collect();

        let (publisher, _) = broadcast::channel(64);
        self.jobs.lock().expect("jobs poisoned").insert(
            spec.id.clone(),
            Finished {
                state: JobState {
                    status: outcome.status,
                    exit_code: outcome.exit_code,
                    error: outcome.error,
                    started_at: Some(now),
                    finished_at: Some(now),
                    steps,
                },
                history,
                publisher,
            },
        );

        Ok(Handle::new(spec.id.clone()))
    }

    async fn poll(&self, handle: &Handle) -> Result<JobState, ExecError> {
        self.jobs
            .lock()
            .expect("jobs poisoned")
            .get(handle.as_str())
            .map(|job| job.state.clone())
            .ok_or_else(|| ExecError::UnknownHandle(handle.clone()))
    }

    async fn logs(&self, handle: &Handle) -> Result<LogTail, ExecError> {
        let jobs = self.jobs.lock().expect("jobs poisoned");
        let job = jobs
            .get(handle.as_str())
            .ok_or_else(|| ExecError::UnknownHandle(handle.clone()))?;

        Ok(LogTail {
            history: job.history.clone(),
            live: job.publisher.subscribe(),
        })
    }

    async fn cancel(&self, handle: &Handle) -> Result<(), ExecError> {
        let mut jobs = self.jobs.lock().expect("jobs poisoned");
        let job = jobs
            .get_mut(handle.as_str())
            .ok_or_else(|| ExecError::UnknownHandle(handle.clone()))?;

        // A mock job is already finished by the time anyone can cancel it, so
        // the cancellation is recorded but does not rewrite a terminal status -
        // which is exactly what a real executor does with a late cancel.
        job.state
            .error
            .get_or_insert_with(|| "cancelled".to_string());

        self.cancelled
            .lock()
            .expect("cancelled poisoned")
            .push(handle.clone());
        Ok(())
    }

    async fn forget(&self, handle: &Handle) -> Result<(), ExecError> {
        self.jobs
            .lock()
            .expect("jobs poisoned")
            .remove(handle.as_str());
        Ok(())
    }
}
