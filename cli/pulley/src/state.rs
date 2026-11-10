//! What pulley remembers about its jobs between runs, so `pulley status` can say what is running now and how the last runs went.
//!
//! Under `~/.config/pulley/state/`, a job has two files: `<id>.live.json` while a run is in progress (what it is doing, how far
//! it has got), and `<id>.runs.jsonl`, one line per finished run, the newest last. Writing is best-effort, like the job log: a
//! disk that cannot take a status file must not stop a backup.

use crate::config::Config;
use quench_core::time::Time;
use quench_serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How many finished runs a job keeps.
pub const KEEP_RUNS: usize = 100;

/// How often a run in progress writes its progress.
const PROGRESS_EVERY: Duration = Duration::from_secs(3);

/// A run in progress.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Live {
    /// `scanning` (the dry run) or `syncing` (the copy).
    pub phase: String,
    /// The process doing it, to tell a run in progress from one that was cut off.
    pub pid: u32,
    /// Seconds since the epoch: when the run began, and when this phase did.
    pub started: i64,
    pub phase_started: i64,
    /// When this was last written.
    pub updated: i64,
    /// Bytes copied so far, how far that is (percent of what rsync knows of), and its current rate.
    pub bytes: u64,
    pub percent: u8,
    pub rate: u64,
}

/// How a run ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// It found something to copy and copied it.
    #[default]
    Synced,
    /// The dry run found nothing to do.
    NoChanges,
    Failed,
}

/// A finished run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub started: i64,
    pub finished: i64,
    /// Seconds the dry run took, and seconds the copy took.
    pub scan_secs: u64,
    pub sync_secs: u64,
    /// What the dry run found.
    pub created: u64,
    pub modified: u64,
    pub deleted: u64,
    /// What the copy transferred.
    pub files: u64,
    pub bytes: u64,
    pub outcome: Outcome,
    /// Why it failed, for [`Outcome::Failed`].
    pub error: String,
}

impl Run {
    /// Bytes per second over the copy; `None` when nothing was copied or it took no time.
    #[must_use]
    pub const fn rate(&self) -> Option<u64> {
        if self.sync_secs == 0 || self.bytes == 0 {
            None
        } else {
            Some(self.bytes / self.sync_secs)
        }
    }
}

/// Seconds since the epoch.
#[must_use]
pub fn now() -> i64 {
    Time::now().timestamp()
}

/// Whether the process `pid` is still running. Where that cannot be told, yes.
#[must_use]
pub fn alive(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        Path::new(&format!("/proc/{pid}")).exists()
    } else {
        true
    }
}

/// A directory of job state.
#[derive(Clone, Debug)]
pub struct Store {
    dir: PathBuf,
}

fn file_name(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl Store {
    /// `~/.config/pulley/state`, or `None` without a home directory.
    #[must_use]
    pub fn global() -> Option<Self> {
        Config::global_config_dir().map(|dir| Self::at(dir.join("state")))
    }

    #[must_use]
    pub const fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn live_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.live.json", file_name(id)))
    }

    fn runs_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.runs.jsonl", file_name(id)))
    }

    /// Writes the progress of a run, replacing the last, by writing beside it and renaming so a reader never sees half.
    pub fn write_live(&self, id: &str, live: &Live) {
        let _ = fs::create_dir_all(&self.dir);
        let path = self.live_path(id);
        let temporary = path.with_extension("json.tmp");
        if fs::write(&temporary, quench_serde::json::to_string(live)).is_ok() {
            let _ = fs::rename(&temporary, &path);
        }
    }

    pub fn clear_live(&self, id: &str) {
        let _ = fs::remove_file(self.live_path(id));
    }

    /// The run in progress, if there is a record of one (which may have been cut off: see [`alive`]).
    #[must_use]
    pub fn live(&self, id: &str) -> Option<Live> {
        let text = fs::read_to_string(self.live_path(id)).ok()?;
        quench_serde::json::from_str(&text).ok()
    }

    /// Adds a finished run, keeping the last [`KEEP_RUNS`].
    pub fn append_run(&self, id: &str, run: &Run) {
        let _ = fs::create_dir_all(&self.dir);
        let path = self.runs_path(id);
        let line = quench_serde::json::to_string(run);
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(file, "{line}");
        }
        let lines = fs::read_to_string(&path).map_or(0, |t| t.lines().count());
        if lines > KEEP_RUNS * 2 {
            let kept: Vec<Run> = self.runs(id, KEEP_RUNS);
            let text: String = kept
                .iter()
                .map(|r| quench_serde::json::to_string(r) + "\n")
                .collect();
            let _ = fs::write(&path, text);
        }
    }

    /// The last `limit` finished runs, oldest first. Lines that cannot be read are skipped.
    #[must_use]
    pub fn runs(&self, id: &str, limit: usize) -> Vec<Run> {
        let Ok(text) = fs::read_to_string(self.runs_path(id)) else {
            return Vec::new();
        };
        let all: Vec<Run> = text
            .lines()
            .filter_map(|line| quench_serde::json::from_str(line).ok())
            .collect();
        let skip = all.len().saturating_sub(limit);
        all.into_iter().skip(skip).collect()
    }
}

/// One run being recorded: `begin` it, move it through its phases, and `finish` it. Dropping it without finishing (a panic, an
/// early return) removes its live record, so nothing is shown running that is not.
#[derive(Debug)]
pub struct Recording {
    store: Option<Store>,
    id: String,
    live: Live,
    run: Run,
    last_write: Instant,
    done: bool,
}

impl Recording {
    /// Starts recording a run of job `id` in the global store (nothing is recorded where there is none).
    #[must_use]
    pub fn begin(id: &str) -> Self {
        Self::begin_in(Store::global(), id)
    }

    #[must_use]
    pub fn begin_in(store: Option<Store>, id: &str) -> Self {
        let at = now();
        let mut recording = Self {
            store,
            id: id.to_string(),
            live: Live {
                phase: "scanning".to_string(),
                pid: std::process::id(),
                started: at,
                phase_started: at,
                updated: at,
                ..Live::default()
            },
            run: Run {
                started: at,
                ..Run::default()
            },
            last_write: Instant::now(),
            done: false,
        };
        recording.write();
        recording
    }

    fn write(&mut self) {
        self.live.updated = now();
        self.last_write = Instant::now();
        if let Some(store) = &self.store {
            store.write_live(&self.id, &self.live);
        }
    }

    /// The dry run is over: what it found, and the copy begins.
    pub fn scanned(&mut self, created: u64, modified: u64, deleted: u64) {
        let at = now();
        self.run.scan_secs = u64::try_from(at - self.live.phase_started).unwrap_or(0);
        self.run.created = created;
        self.run.modified = modified;
        self.run.deleted = deleted;
        self.live.phase = "syncing".to_string();
        self.live.phase_started = at;
        self.write();
    }

    /// How far the copy has got; written at most every few seconds.
    pub fn progress(&mut self, bytes: u64, percent: u8, rate: u64) {
        self.live.bytes = bytes;
        self.live.percent = percent;
        self.live.rate = rate;
        if self.last_write.elapsed() >= PROGRESS_EVERY {
            self.write();
        }
    }

    /// The run ended: records how, and removes the live record.
    pub fn finish(mut self, outcome: Outcome, files: u64, bytes: u64, error: &str) {
        let at = now();
        if outcome == Outcome::Synced
            || (outcome == Outcome::Failed && self.live.phase == "syncing")
        {
            self.run.sync_secs = u64::try_from(at - self.live.phase_started).unwrap_or(0);
        } else if self.live.phase == "scanning" {
            self.run.scan_secs = u64::try_from(at - self.live.phase_started).unwrap_or(0);
        }
        self.run.finished = at;
        self.run.outcome = outcome;
        self.run.files = files;
        self.run.bytes = bytes;
        self.run.error = error.to_string();
        if let Some(store) = &self.store {
            store.append_run(&self.id, &self.run);
            store.clear_live(&self.id);
        }
        self.done = true;
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if !self.done
            && let Some(store) = &self.store
        {
            store.clear_live(&self.id);
        }
    }
}
