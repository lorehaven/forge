//! What the screens show of a backup: a plain copy of the numbers, made by the host from the engine and handed to the
//! app, so the views hold no engine and the words are decided (and tested) here.

use crate::{Engine, Pause, Settings, Status, Totals};

/// One category's line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub key: String,
    pub label: String,
    pub totals: Totals,
}

/// The state of the backup at a moment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub rows: Vec<Row>,
    pub totals: Totals,
    /// Names of the files being sent now.
    pub sending: Vec<String>,
    pub paused: Option<Pause>,
    /// Whether a scan or an upload is going on.
    pub working: bool,
    /// When the backup last looked (epoch milliseconds), if it has.
    pub checked_ms: Option<i64>,
    /// `(used, quota)` in bytes of the storage, when the server says.
    pub quota: Option<(u64, u64)>,
}

impl Snapshot {
    /// The numbers now. `working` is the host's: it knows whether a scan is under way.
    #[must_use]
    pub fn of(
        engine: &Engine,
        settings: &Settings,
        working: bool,
        checked_ms: Option<i64>,
    ) -> Self {
        let rows = settings
            .categories()
            .into_iter()
            .map(|c| Row {
                totals: engine.manifest.totals(Some(&c.key)),
                key: c.key,
                label: c.label,
            })
            .collect::<Vec<_>>();
        let mut totals = Totals::default();
        for r in &rows {
            let t = r.totals;
            totals.pending += t.pending;
            totals.uploading += t.uploading;
            totals.done += t.done;
            totals.failed += t.failed;
            totals.too_large += t.too_large;
            totals.done_bytes += t.done_bytes;
            totals.waiting_bytes += t.waiting_bytes;
        }
        let sending = engine
            .manifest
            .items()
            .filter(|i| i.status == Status::Uploading)
            .take(3)
            .map(|i| i.name.clone())
            .collect();
        Self {
            rows,
            totals,
            sending,
            paused: engine.paused(),
            working,
            checked_ms,
            quota: None,
        }
    }

    /// Files that count: backed up or still to be (those too large for the server are not a task).
    #[must_use]
    pub const fn files(&self) -> usize {
        self.totals.done + self.totals.waiting()
    }

    /// How far along, from 0 to 1 (1 when there is nothing to do).
    #[must_use]
    pub fn fraction(&self) -> f64 {
        let all = self.files();
        if all == 0 {
            1.0
        } else {
            self.totals.done as f64 / all as f64
        }
    }

    /// The first line of the dashboard.
    #[must_use]
    pub fn headline(&self) -> String {
        if let Some(why) = self.paused {
            return why.words().to_string();
        }
        let t = &self.totals;
        if self.files() == 0 {
            return if self.working {
                "Looking for files".into()
            } else {
                "Nothing to back up yet".into()
            };
        }
        if t.pending + t.uploading > 0 {
            return format!("Backing up: {} of {} files", t.done, self.files());
        }
        if t.failed > 0 {
            return format!(
                "{} {} could not be backed up",
                t.failed,
                if t.failed == 1 { "file" } else { "files" }
            );
        }
        format!("All {} files are backed up", t.done)
    }
}

/// A time of day, `HH:MM`, from epoch milliseconds and the zone's offset in minutes.
#[must_use]
pub fn clock(epoch_ms: i64, offset_minutes: i32) -> String {
    let minutes = epoch_ms.div_euclid(60_000) + i64::from(offset_minutes);
    let of_day = minutes.rem_euclid(1440);
    format!("{:02}:{:02}", of_day / 60, of_day % 60)
}

/// Bytes as a person reads them: `950 B`, `12.5 KB`, `3.4 GB`.
#[must_use]
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
