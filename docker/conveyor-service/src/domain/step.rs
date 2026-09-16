//! One step as it was actually executed. Named `StepRecord` (not `Step`) since `pipeline::spec::Step`
//! is what the author wrote, not what happened.

use crate::domain::Status;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepRecord {
    pub id: String,
    pub job_id: String,
    /// Position within the job, from zero. Steps run in this order, always.
    pub ordinal: i32,
    /// `run`, `anvil`, `riveter`, `warehouse`.
    pub kind: String,
    /// The command as it was run, after secrets were redacted out of it.
    pub command: String,
    pub status: Status,
    pub exit_code: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl StepRecord {
    pub fn duration_secs(&self) -> Option<i64> {
        let started = self.started_at?;
        let finished = self.finished_at?;
        Some((finished - started).num_seconds())
    }
}
