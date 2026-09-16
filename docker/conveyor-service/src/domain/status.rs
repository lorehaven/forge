//! One status vocabulary for runs, jobs and steps - three parallel enums would drift.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Accepted and waiting for a worker.
    #[default]
    Queued,
    /// Claimed by a worker and in flight.
    Running,
    Success,
    Failed,
    /// Asked to stop, by a person or by a newer run superseding this one.
    Cancelled,
    /// Never ran: a `when` that evaluated false, or a dependency that failed.
    Skipped,
}

impl Status {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "success" => Some(Self::Success),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }

    /// Whether this is a resting state. A terminal status is never written over.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Success | Self::Failed | Self::Cancelled | Self::Skipped
        )
    }

    /// `Skipped` is deliberately not a failure - a `when`-excluded stage did what the pipeline asked.
    pub const fn is_failure(self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled)
    }

    /// Worst-wins: running beats failure beats cancellation beats skipped-all.
    pub fn rollup(children: impl IntoIterator<Item = Self>) -> Self {
        let mut seen_running = false;
        let mut seen_failed = false;
        let mut seen_cancelled = false;
        let mut seen_success = false;
        let mut any = false;

        for child in children {
            any = true;
            match child {
                Self::Queued | Self::Running => seen_running = true,
                Self::Failed => seen_failed = true,
                Self::Cancelled => seen_cancelled = true,
                Self::Success => seen_success = true,
                Self::Skipped => {}
            }
        }

        match () {
            () if !any => Self::Skipped,
            () if seen_running => Self::Running,
            () if seen_failed => Self::Failed,
            () if seen_cancelled => Self::Cancelled,
            () if seen_success => Self::Success,
            () => Self::Skipped,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
