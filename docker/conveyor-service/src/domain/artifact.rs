//! Something a run produced and left somewhere durable.
//!
//! Conveyor records the reference, not the bytes: a crate version or an image
//! tag lives in warehouse, and copying it into conveyor's database would give
//! the estate two answers to "what did this build publish".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub run_id: String,
    pub job_id: String,
    /// `crate`, `image`, or `file`.
    pub kind: String,
    /// What it is called where it lives - `warehouse-cli`, `forge/sage`.
    pub name: String,
    /// The published version or tag.
    pub version: Option<String>,
    /// Where to fetch it, absolute and directly usable.
    pub uri: String,
    /// Content digest, when the store gives one.
    pub digest: Option<String>,
    pub created_at: DateTime<Utc>,
}
