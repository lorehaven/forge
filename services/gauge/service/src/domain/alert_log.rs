//! Which alerts are open on which host, and when that changes.
//!
//! The store hands this each scrape's alerts; what comes back is only the changes, which the history writes down:
//! an alert that appeared is opened, one that is no longer there is closed. A host that stops answering is itself an
//! alert (`down`), and while it is silent its other alerts stay as they were - they are neither confirmed nor cleared.

use gauge_core::Alert;
use std::collections::{HashMap, HashSet};

/// The key of a host that does not answer.
pub const DOWN: &str = "down";

#[derive(Debug, Clone, PartialEq)]
pub enum AlertChange {
    Opened { host: String, alert: Alert, at: f64 },
    Closed { host: String, key: String, at: f64 },
}

#[derive(Debug, Default)]
pub struct AlertLog {
    open: HashMap<String, HashSet<String>>,
}

impl AlertLog {
    /// A scrape that worked: `alerts` is everything that is wrong now.
    pub fn seen(&mut self, host: &str, at: f64, alerts: &[Alert]) -> Vec<AlertChange> {
        let open = self.open.entry(host.to_string()).or_default();
        let mut changes = Vec::new();
        let now: HashSet<&str> = alerts.iter().map(|a| a.key.as_str()).collect();
        let mut gone: Vec<String> = open
            .iter()
            .filter(|k| !now.contains(k.as_str()))
            .cloned()
            .collect();
        gone.sort();
        for key in gone {
            open.remove(&key);
            changes.push(AlertChange::Closed {
                host: host.to_string(),
                key,
                at,
            });
        }
        for alert in alerts {
            if open.insert(alert.key.clone()) {
                changes.push(AlertChange::Opened {
                    host: host.to_string(),
                    alert: alert.clone(),
                    at,
                });
            }
        }
        changes
    }

    /// A scrape that failed with `error`.
    pub fn silent(&mut self, host: &str, at: f64, error: &str) -> Vec<AlertChange> {
        let open = self.open.entry(host.to_string()).or_default();
        if !open.insert(DOWN.to_string()) {
            return Vec::new();
        }
        vec![AlertChange::Opened {
            host: host.to_string(),
            alert: Alert {
                key: DOWN.into(),
                kind: DOWN.into(),
                detail: error.to_string(),
                severe: true,
            },
            at,
        }]
    }
}
