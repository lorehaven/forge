//! Forklift's logic with no interface and no I/O: where a file goes in the warehouse, and what is known about every
//! file the backup looks after. It builds for the browser and the hosts alike, and its tests run natively and fast.

/// The server a person most likely means: offered on the sign-in until they type another.
pub const DEFAULT_SERVER: &str = "https://ennor.ddns.net";

pub mod engine;
pub mod manifest;
pub mod restore;
pub mod settings;
pub mod snapshot;

pub use snapshot::{Row, Snapshot, bytes, clock};

pub use engine::{Engine, Outcome, Pause, Start, Verdict};

pub use manifest::{
    Found, Item, LARGE_FILE_BYTES, MAX_ATTEMPTS, Manifest, Scanned, Status, Totals, backoff_ms,
};
pub use restore::{
    Fetch, Plan, PlanSummary, Present, Progress, RESTORE_ATTEMPTS, Remote, RestoreView, Restorer,
    base_name, category_of, plan,
};
pub use settings::{
    Category, Folder, PHOTOS, Settings, StorageInfo, VIDEOS, clean_server, folder_slug,
    storage_name,
};

/// The remote key of a file: `{category}/{yyyy}/{mm}/{local_id}-{filename}`.
///
/// The id keeps two files with a reused camera name apart; the date and the name are for a person browsing later.
/// Every segment is cleaned to what warehouse accepts: letters, digits, `.`, `_` and `-`.
#[must_use]
pub fn remote_path(
    category: &str,
    local_id: &str,
    filename: &str,
    year: i32,
    month: u32,
) -> String {
    format!(
        "{category}/{year:04}/{month:02}/{}-{}",
        sanitize(local_id),
        sanitize(filename)
    )
}

/// One path segment with everything unsafe replaced by `_`; an empty one becomes `_`.
#[must_use]
pub fn sanitize(segment: &str) -> String {
    let cleaned: String = segment
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "_".to_string()
    } else {
        cleaned
    }
}

/// The calendar year and month (UTC) of a time in epoch milliseconds, for the path of a file.
#[must_use]
pub const fn year_month(epoch_ms: i64) -> (i32, u32) {
    // Civil-from-days (Howard Hinnant), valid for the whole range a file can have.
    let days = epoch_ms.div_euclid(86_400_000) + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32)
}

/// A short stable id for a file that has none of its own (a document of a picked folder is known by a long URI):
/// 16 hex digits of the FNV-1a hash of `text`.
#[must_use]
pub fn short_id(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}
