//! One step of one job, as it was actually executed.
//!
//! Named `StepRecord` rather than `Step` because the pipeline has a `Step` too
//! (`pipeline::spec`): that one is what the author wrote, this one is what
//! happened. They are different things and confusing them is how a run report
//! ends up showing the template instead of the command.

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
