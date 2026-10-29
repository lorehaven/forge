//! What is known about every file the backup looks after, and what to do next with each.
//!
//! One record per `(category, local_id)`. A file is told apart from itself a moment later by `(size, modified)`:
//! unchanged files are never looked at again, so a scan of a big library is a lookup per file, not a hash.
//!
//! Records live in a plain text file (one line each, tab separated, escaped), written whole to a temporary file and
//! renamed over the old one, so a crash leaves the old manifest or the new one, never half of either.

use std::collections::{BTreeMap, BTreeSet};

/// How many times a file is tried before it is left `Failed` until the next explicit scan.
pub const MAX_ATTEMPTS: u32 = 5;

/// Files larger than this upload only while charging (long 4K video).
pub const LARGE_FILE_BYTES: u64 = 200 * 1024 * 1024;

/// Where a file is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Waiting for its turn (new, changed, or to be retried).
    Pending,
    /// Being sent now. Found in a manifest that was just loaded, it means the app died mid-upload.
    Uploading,
    /// The warehouse has this version of the file.
    Done,
    /// Tried [`MAX_ATTEMPTS`] times; a scan the person asked for tries again.
    Failed,
    /// Larger than the server accepts. Not retried unless the file changes.
    TooLarge,
}

impl Status {
    const fn word(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Uploading => "uploading",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::TooLarge => "too_large",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "pending" => Self::Pending,
            "uploading" => Self::Uploading,
            "done" => Self::Done,
            "failed" => Self::Failed,
            "too_large" => Self::TooLarge,
            _ => return None,
        })
    }
}

/// A file as a source lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// What the source calls it, stable for the life of the file (a MediaStore id).
    pub local_id: String,
    pub name: String,
    /// Where to read it from (a content URI on Android).
    pub uri: String,
    pub size: u64,
    pub modified_ms: i64,
}

/// One file's record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub category: String,
    pub local_id: String,
    pub name: String,
    pub uri: String,
    pub size: u64,
    pub modified_ms: i64,
    pub status: Status,
    pub attempts: u32,
    /// Epoch milliseconds: not before this. Zero is "now".
    pub retry_at_ms: i64,
    /// The last failure, in words a person reads; empty when there is none.
    pub error: String,
    /// The remote path it was sent to; empty until it is.
    pub remote: String,
    /// The last scan did not find it any more (it was deleted from the device): the copy in the storage is all there
    /// is.
    pub gone: bool,
}

impl Item {
    /// Whether this one waits for the device to be charging.
    #[must_use]
    pub const fn is_large(&self) -> bool {
        self.size > LARGE_FILE_BYTES
    }
}

/// What a scan changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Scanned {
    pub added: usize,
    pub changed: usize,
    /// Left as they were: already done, or already waiting.
    pub unchanged: usize,
}

/// Counts by status, for the dashboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub pending: usize,
    pub uploading: usize,
    pub done: usize,
    pub failed: usize,
    pub too_large: usize,
    pub done_bytes: u64,
    pub waiting_bytes: u64,
}

impl Totals {
    /// Files not yet safe: waiting, being sent, or failed.
    #[must_use]
    pub const fn waiting(&self) -> usize {
        self.pending + self.uploading + self.failed
    }
}

/// Every record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    items: BTreeMap<String, Item>,
}

fn key(category: &str, local_id: &str) -> String {
    format!("{category}\u{1f}{local_id}")
}

/// Seconds to wait after the `attempts`-th failure: 30 s, 1 min, 2 min, ... up to an hour.
#[must_use]
pub const fn backoff_ms(attempts: u32) -> i64 {
    let shift = if attempts == 0 { 0 } else { attempts - 1 };
    let shift = if shift > 7 { 7 } else { shift };
    let seconds = 30_i64 << shift;
    (if seconds > 3600 { 3600 } else { seconds }) * 1000
}

impl Manifest {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.items.values()
    }

    #[must_use]
    pub fn get(&self, category: &str, local_id: &str) -> Option<&Item> {
        self.items.get(&key(category, local_id))
    }

    /// Takes what a source found in `category`: new files wait, changed ones wait again, the rest is left alone. A
    /// file that was sending when the app died (`Uploading` in a loaded manifest) goes back to waiting.
    ///
    /// `retry_failed` is the person asking for a backup now: failed files get their attempts back.
    pub fn scan(&mut self, category: &str, found: &[Found], retry_failed: bool) -> Scanned {
        let mut result = Scanned::default();
        for f in found {
            let k = key(category, &f.local_id);
            match self.items.get_mut(&k) {
                None => {
                    result.added += 1;
                    self.items.insert(
                        k,
                        Item {
                            category: category.to_string(),
                            local_id: f.local_id.clone(),
                            name: f.name.clone(),
                            uri: f.uri.clone(),
                            size: f.size,
                            modified_ms: f.modified_ms,
                            status: Status::Pending,
                            attempts: 0,
                            retry_at_ms: 0,
                            error: String::new(),
                            remote: String::new(),
                            gone: false,
                        },
                    );
                }
                Some(item) => {
                    item.gone = false;
                    f.uri.clone_into(&mut item.uri);
                    if item.size != f.size || item.modified_ms != f.modified_ms {
                        result.changed += 1;
                        f.name.clone_into(&mut item.name);
                        item.size = f.size;
                        item.modified_ms = f.modified_ms;
                        item.status = Status::Pending;
                        item.attempts = 0;
                        item.retry_at_ms = 0;
                        item.error.clear();
                    } else if item.status == Status::Failed && retry_failed {
                        item.status = Status::Pending;
                        item.attempts = 0;
                        item.retry_at_ms = 0;
                        result.changed += 1;
                    } else {
                        result.unchanged += 1;
                    }
                }
            }
        }
        result
    }

    /// After a scan of `category` that saw everything there is (not one that failed half way): what it did not find
    /// is gone from the device. A file never sent is forgotten (there is nothing to send any more); one that was sent is
    /// kept, as the record of what the storage holds.
    pub fn mark_missing(&mut self, category: &str, found: &[Found]) {
        let seen: BTreeSet<&str> = found.iter().map(|f| f.local_id.as_str()).collect();
        self.items.retain(|_, i| {
            i.category != category || seen.contains(i.local_id.as_str()) || i.status == Status::Done
        });
        for i in self.items.values_mut().filter(|i| i.category == category) {
            i.gone = !seen.contains(i.local_id.as_str());
        }
    }

    /// Puts files found sending back to waiting (call it once, after loading).
    pub fn recover(&mut self) {
        for item in self.items.values_mut() {
            if item.status == Status::Uploading {
                item.status = Status::Pending;
            }
        }
    }

    /// The next files to send: waiting, not before their time, newest first (what was just taken is what a person
    /// misses most); large files only when `charging`. At most `n`.
    #[must_use]
    pub fn next_batch(&self, now_ms: i64, n: usize, charging: bool) -> Vec<&Item> {
        self.next_batch_where(now_ms, n, charging, |_| true)
    }

    /// [`next_batch`](Self::next_batch), for the categories `allowed` says may send now (a category kept for
    /// Wi-Fi waits on mobile data).
    pub fn next_batch_where(
        &self,
        now_ms: i64,
        n: usize,
        charging: bool,
        allowed: impl Fn(&str) -> bool,
    ) -> Vec<&Item> {
        let mut due: Vec<&Item> = self
            .items
            .values()
            .filter(|i| i.status == Status::Pending && i.retry_at_ms <= now_ms)
            .filter(|i| charging || !i.is_large())
            .filter(|i| allowed(&i.category))
            .collect();
        due.sort_by(|a, b| (b.modified_ms, &b.local_id).cmp(&(a.modified_ms, &a.local_id)));
        due.truncate(n);
        due
    }

    /// The earliest time a waiting file is due, when none is due now: when to look again.
    #[must_use]
    pub fn next_due_ms(&self, now_ms: i64) -> Option<i64> {
        self.items
            .values()
            .filter(|i| i.status == Status::Pending && i.retry_at_ms > now_ms)
            .map(|i| i.retry_at_ms)
            .min()
    }

    pub fn start(&mut self, category: &str, local_id: &str) {
        if let Some(i) = self.items.get_mut(&key(category, local_id)) {
            i.status = Status::Uploading;
        }
    }

    /// Where the file goes in the storage: fixed at the first send, so an edited file replaces its own copy.
    pub fn set_remote(&mut self, category: &str, local_id: &str, remote: &str) {
        if let Some(i) = self.items.get_mut(&key(category, local_id)) {
            remote.clone_into(&mut i.remote);
        }
    }

    pub fn done(&mut self, category: &str, local_id: &str, remote: &str) {
        if let Some(i) = self.items.get_mut(&key(category, local_id)) {
            i.status = Status::Done;
            i.attempts = 0;
            i.error.clear();
            remote.clone_into(&mut i.remote);
        }
    }

    /// The upload failed: wait and try again, or give up for now after [`MAX_ATTEMPTS`].
    pub fn failed(&mut self, category: &str, local_id: &str, now_ms: i64, error: &str) {
        if let Some(i) = self.items.get_mut(&key(category, local_id)) {
            i.attempts += 1;
            error.clone_into(&mut i.error);
            if i.attempts >= MAX_ATTEMPTS {
                i.status = Status::Failed;
            } else {
                i.status = Status::Pending;
                i.retry_at_ms = now_ms + backoff_ms(i.attempts);
            }
        }
    }

    /// The server will not take it, at this size.
    pub fn too_large(&mut self, category: &str, local_id: &str) {
        if let Some(i) = self.items.get_mut(&key(category, local_id)) {
            i.status = Status::TooLarge;
            "larger than the server accepts".clone_into(&mut i.error);
        }
    }

    /// Back to waiting, without counting a failure (the run was stopped, or the quota is full).
    pub fn release(&mut self, category: &str, local_id: &str) {
        if let Some(i) = self.items.get_mut(&key(category, local_id))
            && i.status == Status::Uploading
        {
            i.status = Status::Pending;
        }
    }

    /// Waiting again from the start, as if the file were new (it is not in the storage that is now in use).
    pub fn reset(&mut self, category: &str, local_id: &str) {
        if let Some(i) = self.items.get_mut(&key(category, local_id)) {
            i.status = Status::Pending;
            i.attempts = 0;
            i.retry_at_ms = 0;
            i.error.clear();
            i.remote.clear();
        }
    }

    /// Forgets what a category held (its folder was removed from the backup). The warehouse keeps its copies.
    pub fn forget_category(&mut self, category: &str) {
        self.items.retain(|_, i| i.category != category);
    }

    #[must_use]
    pub fn totals(&self, category: Option<&str>) -> Totals {
        let mut t = Totals::default();
        for i in self
            .items
            .values()
            .filter(|i| category.is_none_or(|c| c == i.category))
        {
            match i.status {
                Status::Pending => t.pending += 1,
                Status::Uploading => t.uploading += 1,
                Status::Done => t.done += 1,
                Status::Failed => t.failed += 1,
                Status::TooLarge => t.too_large += 1,
            }
            match i.status {
                Status::Done => t.done_bytes += i.size,
                Status::Pending | Status::Uploading | Status::Failed => t.waiting_bytes += i.size,
                Status::TooLarge => {}
            }
        }
        t
    }

    /// The records as text, one line each.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::from("forklift-manifest 1\n");
        for i in self.items.values() {
            let fields = [
                escape(&i.category),
                escape(&i.local_id),
                escape(&i.name),
                escape(&i.uri),
                i.size.to_string(),
                i.modified_ms.to_string(),
                i.status.word().to_string(),
                i.attempts.to_string(),
                i.retry_at_ms.to_string(),
                escape(&i.error),
                escape(&i.remote),
                if i.gone { "1" } else { "0" }.to_string(),
            ];
            out.push_str(&fields.join("\t"));
            out.push('\n');
        }
        out
    }

    /// Reads what [`to_text`](Self::to_text) wrote. A line that does not parse is skipped (the file is ours, so that
    /// means damage, and losing one record only means it is looked at again by the next scan).
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut manifest = Self::new();
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            // An older file has no `gone` column.
            if f.len() != 11 && f.len() != 12 {
                continue;
            }
            let (Ok(size), Ok(modified_ms), Some(status), Ok(attempts), Ok(retry_at_ms)) = (
                f[4].parse(),
                f[5].parse(),
                Status::parse(f[6]),
                f[7].parse(),
                f[8].parse(),
            ) else {
                continue;
            };
            let item = Item {
                category: unescape(f[0]),
                local_id: unescape(f[1]),
                name: unescape(f[2]),
                uri: unescape(f[3]),
                size,
                modified_ms,
                status,
                attempts,
                retry_at_ms,
                error: unescape(f[9]),
                remote: unescape(f[10]),
                gone: f.get(11) == Some(&"1"),
            };
            manifest
                .items
                .insert(key(&item.category, &item.local_id), item);
        }
        manifest
    }
}

pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}
