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

/// How far back a chart looks. `Live` is the last hour at the scrape interval, from memory; the others are
/// the stored one-minute samples, averaged down to a few hundred points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    #[default]
    Live,
    Day,
    Week,
    Month,
}

impl Range {
    pub const ALL: [Self; 4] = [Self::Live, Self::Day, Self::Week, Self::Month];

    /// The name in URLs and on the buttons.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Live => "1h",
            Self::Day => "24h",
            Self::Week => "7d",
            Self::Month => "30d",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == text)
    }

    /// Seconds the chart covers.
    #[must_use]
    pub const fn window_secs(self) -> u64 {
        match self {
            Self::Live => 3600,
            Self::Day => 86_400,
            Self::Week => 7 * 86_400,
            Self::Month => 30 * 86_400,
        }
    }

    /// Seconds each point stands for: 288, 336 and 360 points.
    #[must_use]
    pub const fn step_secs(self) -> u64 {
        match self {
            Self::Live => 10,
            Self::Day => 300,
            Self::Week => 1800,
            Self::Month => 7200,
        }
    }
}

/// One host's stored history: `points` is evenly spaced, `step_secs` apart, oldest first, with a gap where nothing
/// was stored (the host did not answer, or Gauge was not running).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostHistory {
    pub name: String,
    /// `24h`, `7d` or `30d`.
    pub range: String,
    pub step_secs: u64,
    pub points: Vec<Point>,
}

/// One filesystem's growth and when it fills, from its stored history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskForecast {
    pub host: String,
    /// The mount point, or the device when the agent does not say.
    pub mount: String,
    pub size: f64,
    pub used: f64,
    /// Bytes added a day; `None` until there is enough history.
    pub per_day: Option<f64>,
    /// Days until it is full at that rate; `None` when it is not growing, or there is not enough history.
    pub days_left: Option<f64>,
}

/// One alert as the log keeps it: when it opened and, once it is over, when it closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertEvent {
    pub host: String,
    /// What the alert is of (see `Alert::key`); `down` is a host that stopped answering.
    pub key: String,
    pub kind: String,
    /// The wording as it was when the alert opened.
    pub detail: String,
    pub severe: bool,
    /// Unix seconds.
    pub opened: f64,
    /// `None` while it is still going.
    pub closed: Option<f64>,
}
