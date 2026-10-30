//! The latest view and a short history per host, in memory. A restart starts the charts over; Gauge is a
//! window onto what the agents report, not a time-series database.

use super::config::{Kind, Target};
use super::model::{Reading, View};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Points kept per host: an hour at the default 10 s interval.
pub const HISTORY: usize = 360;

pub use gauge_core::Point;

#[derive(Debug, Clone)]
pub struct Host {
    pub name: String,
    pub kind: Kind,
    pub url: String,
    pub last: Option<Reading>,
    pub view: Option<View>,
    pub history: VecDeque<Point>,
    /// Why the last scrape failed; `None` while the host answers.
    pub error: Option<String>,
    pub last_ok: Option<f64>,
}

impl Host {
    #[must_use]
    pub const fn up(&self) -> bool {
        self.error.is_none() && self.view.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Store {
    hosts: Arc<Mutex<Vec<Host>>>,
}

impl Store {
    #[must_use]
    pub fn new(targets: &[Target]) -> Self {
        let hosts = targets
            .iter()
            .map(|t| Host {
                name: t.name.clone(),
                kind: t.kind,
                url: t.url.clone(),
                last: None,
                view: None,
                history: VecDeque::new(),
                error: Some("waiting for the first scrape".into()),
                last_ok: None,
            })
            .collect();
        Self {
            hosts: Arc::new(Mutex::new(hosts)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Host>> {
        self.hosts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn all(&self) -> Vec<Host> {
        self.lock().clone()
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<Host> {
        self.lock().iter().find(|h| h.name == name).cloned()
    }

    pub fn record(&self, name: &str, result: Result<Reading, String>) {
        let mut hosts = self.lock();
        let Some(host) = hosts.iter_mut().find(|h| h.name == name) else {
            return;
        };
        match result {
            Err(error) => {
                host.error = Some(error);
                // Forget the last reading: its counters are hours old by the time the host is back, and a
                // rate over that gap would be an average of nothing. The charts get one break instead.
                host.last = None;
                if let Some(at) = host
                    .last_ok
                    .filter(|_| host.history.back().is_some_and(|p| !p.is_gap()))
                {
                    host.history.push_back(Point::gap(at));
                }
            }
            Ok(reading) => {
                let view = View::from_readings(host.last.as_ref(), &reading);
                // The first scrape has no rates yet; wait for the second before drawing a point.
                if host.last.is_some() {
                    host.history.push_back(point(&view));
                    while host.history.len() > HISTORY {
                        host.history.pop_front();
                    }
                }
                host.last_ok = Some(reading.at);
                host.last = Some(reading);
                host.view = Some(view);
                host.error = None;
            }
        }
        drop(hosts);
    }
}

fn point(v: &View) -> Point {
    let sum = |rates: &[super::model::Rate], b: bool| -> f64 {
        rates
            .iter()
            .map(|r| if b { r.b_per_sec } else { r.a_per_sec })
            .sum()
    };
    Point {
        t: v.at,
        cpu: v.cpu_pct,
        mem: v.mem_pct(),
        rx: Some(sum(&v.nics, false)),
        tx: Some(sum(&v.nics, true)),
        read: Some(sum(&v.disks, false)),
        write: Some(sum(&v.disks, true)),
        gpu: v.gpus.iter().filter_map(|g| g.busy_pct).reduce(f64::max),
        vram: v
            .gpus
            .iter()
            .filter_map(|g| {
                g.vram_used
                    .zip(g.vram_total)
                    .filter(|(_, t)| *t > 0.0)
                    .map(|(u, t)| u / t * 100.0)
            })
            .reduce(f64::max),
    }
}
