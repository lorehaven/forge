//! One job of one run: a named unit inside a stage, executed as a whole.
//!
//! `stage` and `needs` are copied out of the pipeline onto the record rather
//! than looked up later. The `.conveyor.toml` that produced them lives at a
//! commit, and reading it again to render a two-month-old run would mean
//! checking that commit out again.

use crate::domain::Status;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub run_id: String,
    pub stage: String,
    pub name: String,
    /// Stages this job's stage waited on, as the pipeline declared them.
    pub needs: Vec<String>,
    pub status: Status,
    /// Exit code of the first step that failed, or of the last step when they
    /// all passed. `None` while running, and for a job that never started.
    pub exit_code: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    /// Why the job ended, when there is more to say than the exit code - a
    /// timeout, a cancellation, an executor that could not start it.
    pub error: Option<String>,
}

impl Job {
    /// `build/cargo`, the form used in logs and in the UI's job list.
    pub fn qualified_name(&self) -> String {
        format!("{}/{}", self.stage, self.name)
    }

    pub fn duration_secs(&self) -> Option<i64> {
        let started = self.started_at?;
        let finished = self.finished_at?;
        Some((finished - started).num_seconds())
    }
}
