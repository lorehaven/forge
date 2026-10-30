//! What `GET /api/v1/hosts` and `GET /api/v1/hosts/{name}` answer.

use quench_serde::{Deserialize, Serialize};

use crate::model::{Point, View};

/// One host, as the list shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostSummary {
    pub name: String,
    /// `pcp` or `node_exporter`.
    pub kind: String,
    pub url: String,
    pub up: bool,
    /// Why the last scrape failed; `None` while the host answers.
    pub error: Option<String>,
    /// Unix seconds of the last scrape that worked.
    pub last_ok: Option<f64>,
    /// The latest numbers; `None` before the first scrape.
    pub view: Option<View>,
}

/// One host as the list shows it: the summary, and the CPU history its card draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostCard {
    #[serde(flatten)]
    pub host: HostSummary,
    /// CPU busy percentage, oldest first; `None` where the host did not answer.
    pub cpu: Vec<Option<f64>>,
}

/// One host with its history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostDetail {
    #[serde(flatten)]
    pub host: HostSummary,
    pub history: Vec<Point>,
}
