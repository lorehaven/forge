//! Knowing what the server already has, and bringing back what the device lacks.
//!
//! Two jobs share one question, "is this stored file the same as that file here?": a fresh install must not send again
//! what the server holds (reconciliation), and a restore must not fetch what the device has. The same file can be told
//! by its path, or (when the device gave it a new id, as after a restore or a reset) by its category, name and size.

use std::collections::{BTreeMap, BTreeSet};

use crate::manifest::{Manifest, Status};
use crate::settings::Folder;
use crate::{remote_path, sanitize, year_month};

/// A stored file as the server lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub path: String,
    pub size: u64,
}

/// The category of a stored file: the first segment of its path.
#[must_use]
pub fn category_of(path: &str) -> &str {
    path.split('/').next().unwrap_or_default()
}

/// The name a file had on the device, from its stored name `{id}-{name}`.
#[must_use]
pub fn base_name(path: &str) -> &str {
    let last = path.rsplit('/').next().unwrap_or_default();
    last.split_once('-').map_or(last, |(_, name)| name)
}

/// `(category, name, size)`: how a file is told when its id is not the same.
type Identity = (String, String, u64);

fn identity(remote: &Remote) -> Identity {
    (
        category_of(&remote.path).to_string(),
        base_name(&remote.path).to_string(),
        remote.size,
    )
}

/// What the device has, as the remote paths and identities of its files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Present {
    paths: BTreeSet<String>,
    identities: BTreeSet<Identity>,
}

impl Present {
    /// Whether the stored file is on the device.
    #[must_use]
    pub fn has(&self, remote: &Remote) -> bool {
        self.paths.contains(&remote.path) || self.identities.contains(&identity(remote))
    }
}

impl Manifest {
    /// What this device holds, from every file the scans found: the path it was sent to, or would be.
    #[must_use]
    pub fn present(&self) -> Present {
        let mut present = Present::default();
        for i in self.items().filter(|i| !i.gone) {
            let path = if i.remote.is_empty() {
                let (year, month) = year_month(i.modified_ms);
                remote_path(&i.category, &i.local_id, &i.name, year, month)
            } else {
                i.remote.clone()
            };
            present
                .identities
                .insert((i.category.clone(), sanitize(&i.name), i.size));
            present.paths.insert(path);
        }
        present
    }

    /// Takes the server's list: a file found on the device that the server already holds (the same path and size, or
    /// the same category, name and size) is marked done instead of being sent again. Only files never sent are looked
    /// at. Returns how many were recognised.
    pub fn reconcile(&mut self, known: &[Remote]) -> usize {
        let by_path: BTreeMap<&str, u64> =
            known.iter().map(|r| (r.path.as_str(), r.size)).collect();
        let by_identity: BTreeMap<Identity, &str> = known
            .iter()
            .map(|r| (identity(r), r.path.as_str()))
            .collect();
        let fresh: Vec<(String, String, String, String)> = self
            .items()
            .filter(|i| i.status == Status::Pending && i.remote.is_empty() && i.attempts == 0)
            .map(|i| {
                let (year, month) = year_month(i.modified_ms);
                (
                    i.category.clone(),
                    i.local_id.clone(),
                    remote_path(&i.category, &i.local_id, &i.name, year, month),
                    sanitize(&i.name),
                )
            })
            .collect();
        let mut recognised = 0;
        for (category, id, path, name) in fresh {
            let size = self.get(&category, &id).map_or(0, |i| i.size);
            let found = if by_path.get(path.as_str()) == Some(&size) {
                Some(path)
            } else {
                by_identity
                    .get(&(category.clone(), name, size))
                    .map(|p| (*p).to_string())
            };
            if let Some(remote) = found {
                self.done(&category, &id, &remote);
                recognised += 1;
            }
        }
        recognised
    }
}

/// What a restore would do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The files to fetch.
    pub files: Vec<Remote>,
    pub bytes: u64,
    /// Left out because the device has them.
    pub present: usize,
    /// Left out because there is nowhere to put them (a folder that is no longer part of the backup).
    pub no_target: usize,
}

/// The files the device lacks and has a place for: photos and videos go to the media library, a folder's files to that
/// folder when it is still part of the backup.
#[must_use]
pub fn plan(remote: &[Remote], present: &Present, folders: &[Folder]) -> Plan {
    let mut plan = Plan::default();
    for file in remote {
        if present.has(file) {
            plan.present += 1;
            continue;
        }
        let category = category_of(&file.path);
        let target = category == crate::PHOTOS
            || category == crate::VIDEOS
            || folders.iter().any(|f| f.category() == category);
        if !target {
            plan.no_target += 1;
            continue;
        }
        plan.bytes += file.size;
        plan.files.push(file.clone());
    }
    plan
}

impl Plan {
    /// The numbers a screen shows.
    #[must_use]
    pub fn summary(&self) -> PlanSummary {
        PlanSummary {
            files: self.files.len(),
            bytes: self.bytes,
            present: self.present,
            no_target: self.no_target,
        }
    }
}

/// What a restore would do, in numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanSummary {
    pub files: usize,
    pub bytes: u64,
    pub present: usize,
    pub no_target: usize,
}

/// Where a restore is, for the screen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RestoreView {
    #[default]
    Idle,
    /// Reading what is stored.
    Planning,
    /// What there is to restore, waiting for a yes.
    Plan(PlanSummary),
    Running(Progress),
    /// Over: how it went (stopped by the person, or finished).
    Done(Progress),
    /// Could not even start (for example, the server could not be reached).
    Failed(String),
}

/// How many times a file is fetched before it is given up on.
pub const RESTORE_ATTEMPTS: u32 = 3;

/// One download for the host to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    pub id: String,
    pub file: Remote,
}

/// The progress of a restore.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
}

/// Runs a restore: a few files at a time, each tried a few times.
#[derive(Debug, Default)]
pub struct Restorer {
    queue: Vec<Remote>,
    active: BTreeMap<String, (Remote, u32)>,
    retry: Vec<(Remote, u32)>,
    progress: Progress,
    next: u64,
    concurrency: usize,
    stopped: bool,
}

impl Restorer {
    #[must_use]
    pub fn new(mut files: Vec<Remote>, concurrency: usize) -> Self {
        // The queue is taken from the end: the first of the list goes first.
        files.reverse();
        let total = files.len();
        Self {
            queue: files,
            progress: Progress {
                total,
                ..Progress::default()
            },
            concurrency: concurrency.clamp(1, 4),
            ..Self::default()
        }
    }

    #[must_use]
    pub const fn progress(&self) -> Progress {
        self.progress
    }

    /// Whether everything is done, given up on, or stopped.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.active.is_empty() && (self.stopped || (self.queue.is_empty() && self.retry.is_empty()))
    }

    /// The downloads to start now.
    pub fn start(&mut self) -> Vec<Fetch> {
        let mut starts = Vec::new();
        while !self.stopped && self.active.len() < self.concurrency {
            let Some((file, attempts)) = self
                .retry
                .pop()
                .or_else(|| self.queue.pop().map(|f| (f, 0)))
            else {
                break;
            };
            self.next += 1;
            let id = format!("r{}", self.next);
            self.active.insert(id.clone(), (file.clone(), attempts));
            starts.push(Fetch { id, file });
        }
        starts
    }

    /// A download ended.
    pub fn finish(&mut self, id: &str, ok: bool) {
        let Some((file, attempts)) = self.active.remove(id) else {
            return;
        };
        if ok {
            self.progress.done += 1;
        } else if attempts + 1 >= RESTORE_ATTEMPTS {
            self.progress.failed += 1;
        } else {
            self.retry.push((file, attempts + 1));
        }
    }

    /// Nothing more is started; what runs is given up (the host cancels it).
    pub fn stop(&mut self) -> Vec<String> {
        self.stopped = true;
        let ids: Vec<String> = self.active.keys().cloned().collect();
        self.active.clear();
        ids
    }
}
