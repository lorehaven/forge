//! One job of one run. `stage`/`needs` are copied from the pipeline onto the record rather than
//! looked up later - re-reading a two-month-old commit's `.conveyor.toml` would mean re-checking it out.

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
    /// First failing step's exit code, or the last step's if all passed. `None` while running/unstarted.
    pub exit_code: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    /// Why the job ended, when there's more to say than the exit code (timeout, cancellation, ...).
    pub error: Option<String>,
    /// The run this result was carried over from on a restart. `None` for a job that actually ran here.
    pub reused_from_run: Option<String>,
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
