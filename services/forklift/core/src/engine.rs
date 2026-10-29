//! Sends the waiting files, a few at a time, and decides what each answer means. It does no I/O: the host starts the
//! uploads it is told to start and reports how each ended.

use std::collections::BTreeMap;

use crate::{Manifest, remote_path, year_month};

/// Why nothing more is sent until the person (or the passage of time) changes something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pause {
    /// The storage is full.
    Quota,
    /// The session is no good: sign in again.
    SignedOut,
    /// The server has no storage of that name.
    NoStorage,
    /// The window is in the background: the background run has the backup.
    Hidden,
    /// The server knows this account but does not let it write to this storage.
    Forbidden,
}

impl Pause {
    /// What a screen says.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::Quota => "the storage is full: free some space on the server, then back up again",
            Self::SignedOut => "the session ended: sign in again",
            Self::NoStorage => "the server has no storage with that name: check it in the settings",
            Self::Hidden => "the backup goes on in the background",
            Self::Forbidden => {
                "this account may not write to this storage: ask for write access to it in gatehouse, or choose another storage"
            }
        }
    }
}

/// One upload for the host to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    /// Names this upload in the host's events.
    pub upload_id: String,
    pub uri: String,
    /// The path in the storage.
    pub remote: String,
}

/// How an upload ended, as the host sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The server answered.
    Http(u16),
    /// The transfer broke before an answer.
    Failed(String),
}

/// What the host does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The file is safe.
    Done,
    /// Failed or deferred; the manifest says when it is tried again.
    Retry,
    /// The token was refused: get a fresh one, then pump again.
    Reauthenticate,
    /// Nothing more is sent.
    Paused(Pause),
}

/// The scheduler.
#[derive(Debug)]
pub struct Engine {
    pub manifest: Manifest,
    active: BTreeMap<String, (String, String)>,
    next: u64,
    concurrency: usize,
    auth_refusals: u32,
    paused: Option<Pause>,
}

impl Engine {
    #[must_use]
    pub fn new(manifest: Manifest, concurrency: usize) -> Self {
        Self {
            manifest,
            active: BTreeMap::new(),
            next: 0,
            concurrency: concurrency.clamp(1, 5),
            auth_refusals: 0,
            paused: None,
        }
    }

    /// Uploads running now.
    #[must_use]
    pub fn running(&self) -> usize {
        self.active.len()
    }

    #[must_use]
    pub const fn paused(&self) -> Option<Pause> {
        self.paused
    }

    /// Nothing more is sent until [`resume`](Self::resume); what is in flight waits again.
    pub fn pause_for(&mut self, why: Pause) {
        self.stop();
        self.paused = Some(why);
    }

    /// Takes another copy of the manifest (the background run wrote a newer one while this program was paused).
    pub fn replace_manifest(&mut self, manifest: Manifest) {
        self.manifest = manifest;
    }

    /// The uploads running now, by the names the host knows them by.
    #[must_use]
    pub fn active_ids(&self) -> Vec<String> {
        self.active.keys().cloned().collect()
    }

    /// Lets sending start again (the person fixed what paused it).
    pub const fn resume(&mut self) {
        self.paused = None;
        self.auth_refusals = 0;
    }

    /// The uploads to start now: up to the free slots, newest files first.
    pub fn pump(&mut self, now_ms: i64, charging: bool) -> Vec<Start> {
        self.pump_where(now_ms, charging, |_| true)
    }

    /// [`pump`](Self::pump), for the categories `allowed` says may send now.
    pub fn pump_where(
        &mut self,
        now_ms: i64,
        charging: bool,
        allowed: impl Fn(&str) -> bool,
    ) -> Vec<Start> {
        if self.paused.is_some() {
            return Vec::new();
        }
        let free = self.concurrency.saturating_sub(self.active.len());
        let chosen: Vec<(String, String, String, String, i64, String)> = self
            .manifest
            .next_batch_where(now_ms, free, charging, allowed)
            .into_iter()
            .map(|i| {
                (
                    i.category.clone(),
                    i.local_id.clone(),
                    i.name.clone(),
                    i.uri.clone(),
                    i.modified_ms,
                    i.remote.clone(),
                )
            })
            .collect();
        let mut starts = Vec::new();
        for (category, local_id, name, uri, modified, known) in chosen {
            // Where a file went the first time is where it stays, even if the file is edited and moves in time.
            let remote = if known.is_empty() {
                let (year, month) = year_month(modified);
                remote_path(&category, &local_id, &name, year, month)
            } else {
                known
            };
            self.next += 1;
            let upload_id = format!("u{}", self.next);
            self.manifest.start(&category, &local_id);
            self.manifest.set_remote(&category, &local_id, &remote);
            self.active.insert(upload_id.clone(), (category, local_id));
            starts.push(Start {
                upload_id,
                uri,
                remote,
            });
        }
        starts
    }

    /// An upload ended.
    pub fn finish(&mut self, upload_id: &str, outcome: &Outcome, now_ms: i64) -> Verdict {
        let Some((category, local_id)) = self.active.remove(upload_id) else {
            return Verdict::Retry;
        };
        let remote = self
            .manifest
            .get(&category, &local_id)
            .map(|i| i.remote.clone())
            .unwrap_or_default();
        match outcome {
            Outcome::Http(200..=299) => {
                self.auth_refusals = 0;
                self.manifest.done(&category, &local_id, &remote);
                Verdict::Done
            }
            Outcome::Http(413) => {
                self.manifest.too_large(&category, &local_id);
                Verdict::Retry
            }
            Outcome::Http(507) => {
                self.manifest.release(&category, &local_id);
                self.pause(Pause::Quota)
            }
            Outcome::Http(404) => {
                self.manifest.release(&category, &local_id);
                self.pause(Pause::NoStorage)
            }
            // A refusal of the person, not of the token: signing in again would change nothing.
            Outcome::Http(403) => {
                self.manifest.release(&category, &local_id);
                self.pause(Pause::Forbidden)
            }
            Outcome::Http(401) => {
                self.manifest.release(&category, &local_id);
                self.auth_refusals += 1;
                // One refresh may fix a stale token; a second refusal right after means the session is gone.
                if self.auth_refusals > 2 {
                    self.pause(Pause::SignedOut)
                } else {
                    Verdict::Reauthenticate
                }
            }
            Outcome::Http(code) => {
                self.manifest.failed(
                    &category,
                    &local_id,
                    now_ms,
                    &format!("the server answered {code}"),
                );
                Verdict::Retry
            }
            Outcome::Failed(why) => {
                self.manifest.failed(&category, &local_id, now_ms, why);
                Verdict::Retry
            }
        }
    }

    fn pause(&mut self, why: Pause) -> Verdict {
        self.paused = Some(why);
        Verdict::Paused(why)
    }

    /// The run is stopped (the app closes, the constraints no longer hold): what was in flight waits again, and no
    /// failure is counted.
    pub fn stop(&mut self) {
        let active = std::mem::take(&mut self.active);
        for (category, local_id) in active.into_values() {
            self.manifest.release(&category, &local_id);
        }
    }
}
