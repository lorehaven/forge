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
    /// How many files a scan under way has looked at so far (0 when none is).
    pub looked_at: usize,
    /// Whether the files have been looked at since the app started. Until then "nothing to back up" is not known to be
    /// true (the manifest may still be loading, the first scan not done), so it is not said.
    pub settled: bool,
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
        let mut by = engine.manifest.totals_by_category();
        let rows = settings
            .categories()
            .into_iter()
            .map(|c| Row {
                totals: by.remove(&c.key).unwrap_or_default(),
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
            looked_at: 0,
            settled: false,
        }
    }

    /// The snapshot as text, for the screen to show at once at the next start (before the manifest is read and the
    /// files are looked at again). Plain lines; [`from_text`](Self::from_text) reads them back.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::from("forklift-snapshot 1\n");
        let totals = |t: &Totals| {
            format!(
                "{} {} {} {} {} {} {}",
                t.pending,
                t.uploading,
                t.done,
                t.failed,
                t.too_large,
                t.done_bytes,
                t.waiting_bytes
            )
        };
        out.push_str(&format!("total {}\n", totals(&self.totals)));
        for r in &self.rows {
            out.push_str(&format!(
                "row {}\t{}\t{}\n",
                crate::manifest::escape(&r.key),
                crate::manifest::escape(&r.label),
                totals(&r.totals)
            ));
        }
        if let Some(at) = self.checked_ms {
            out.push_str(&format!("checked {at}\n"));
        }
        if let Some((used, quota)) = self.quota {
            out.push_str(&format!("quota {used} {quota}\n"));
        }
        out
    }

    /// Reads what [`to_text`](Self::to_text) wrote; anything that does not parse gives the empty snapshot.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        fn totals(words: &str) -> Option<Totals> {
            let mut n = words.split(' ');
            Some(Totals {
                pending: n.next()?.parse().ok()?,
                uploading: n.next()?.parse().ok()?,
                done: n.next()?.parse().ok()?,
                failed: n.next()?.parse().ok()?,
                too_large: n.next()?.parse().ok()?,
                done_bytes: n.next()?.parse().ok()?,
                waiting_bytes: n.next()?.parse().ok()?,
            })
        }
        let mut shot = Self::default();
        let mut lines = text.lines();
        if lines.next() != Some("forklift-snapshot 1") {
            return shot;
        }
        for line in lines {
            if let Some(rest) = line.strip_prefix("total ") {
                let Some(t) = totals(rest) else {
                    return Self::default();
                };
                shot.totals = t;
            } else if let Some(rest) = line.strip_prefix("row ") {
                let f: Vec<&str> = rest.split('\t').collect();
                let (Some(&key), Some(&label), Some(t)) =
                    (f.first(), f.get(1), f.get(2).and_then(|w| totals(w)))
                else {
                    return Self::default();
                };
                shot.rows.push(Row {
                    key: crate::manifest::unescape(key),
                    label: crate::manifest::unescape(label),
                    totals: t,
                });
            } else if let Some(rest) = line.strip_prefix("checked ") {
                shot.checked_ms = rest.parse().ok();
            } else if let Some(rest) = line.strip_prefix("quota ") {
                let mut n = rest.split(' ').filter_map(|w| w.parse().ok());
                if let (Some(used), Some(quota)) = (n.next(), n.next()) {
                    shot.quota = Some((used, quota));
                }
            }
        }
        shot
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
            // Nothing is known yet: an empty bar, not a full one.
            if self.settled { 1.0 } else { 0.0 }
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
            return if self.working || !self.settled {
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
