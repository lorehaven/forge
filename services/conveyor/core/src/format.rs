//! Times and sizes as a page shows them. The clock is an argument: the browser's and a test's differ.

use quench_core::time::Time;

/// The coarsest still-informative unit; exact times live elsewhere.
#[must_use]
pub fn relative(when: Time, now: Time) -> String {
    let seconds = (now - when).num_seconds();

    // A clock skew between replicas can put a timestamp slightly in the future.
    if seconds < 0 {
        return "just now".to_string();
    }

    match seconds {
        0..=44 => "just now".to_string(),
        45..=5399 => format!("{}m ago", (seconds + 30) / 60),
        5400..=86399 => format!("{}h ago", (seconds + 1800) / 3600),
        _ => format!("{}d ago", (seconds + 43200) / 86400),
    }
}

/// How long something took.
#[must_use]
pub fn duration(seconds: i64) -> String {
    if seconds < 0 {
        return "-".to_string();
    }
    if seconds < 60 {
        return format!("{seconds}s");
    }

    let minutes = seconds / 60;
    let rest = seconds % 60;
    if minutes < 60 {
        return format!("{minutes}m {rest}s");
    }
    format!("{}h {}m", minutes / 60, minutes % 60)
}

/// The gap between two instants, when both are known; "so far" while it has not finished.
#[must_use]
pub fn elapsed(from: Option<Time>, to: Option<Time>, now: Time) -> String {
    match (from, to) {
        (Some(from), Some(to)) => duration((to - from).num_seconds()),
        (Some(from), None) => format!("{} so far", duration((now - from).num_seconds())),
        _ => "-".to_string(),
    }
}

/// `sha256:0123abc…`: enough to compare by eye, short enough to sit in a row.
#[must_use]
pub fn short_digest(digest: &str) -> String {
    match digest.split_once(':') {
        Some((algorithm, hex)) => {
            let head: String = hex.chars().take(12).collect();
            format!("{algorithm}:{head}…")
        }
        None => digest.to_string(),
    }
}

/// `2026-10-29 10:20 UTC`.
#[must_use]
pub fn stamp(when: Time) -> String {
    when.format("%Y-%m-%d %H:%M UTC")
}

/// The pages of a history of `total` runs; at least 1, so an empty one still has a "page 1 of 1".
#[must_use]
pub fn page_count(total: i64, page_size: i64) -> u32 {
    if total <= 0 {
        return 1;
    }
    let page_size = page_size.max(1);
    let pages = (total + page_size - 1) / page_size;
    u32::try_from(pages).unwrap_or(u32::MAX).max(1)
}

/// One path segment, percent-encoded.
#[must_use]
pub fn segment(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}
