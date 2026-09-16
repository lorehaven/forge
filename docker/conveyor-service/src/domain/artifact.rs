//! Something a run produced. Conveyor records the reference, not the bytes - the artifact itself lives in warehouse.

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
