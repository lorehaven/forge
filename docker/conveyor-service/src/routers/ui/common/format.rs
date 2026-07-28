//! Rendering the small things: times, durations, sizes.

use chrono::{DateTime, Utc};

/// How long ago, in the coarsest unit that is still informative.
///
/// A build log is read minutes after it ran, so "3m ago" answers the question
/// a timestamp makes the reader work out. Exact times are in the title
/// attribute wherever this is used.
pub fn relative(when: DateTime<Utc>) -> String {
    let seconds = (Utc::now() - when).num_seconds();

    // A clock skew between replicas can put a timestamp slightly in the future;
    // "in -2s" would be worse than rounding it to now.
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

/// The gap between two instants, when both are known.
pub fn elapsed(from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> String {
    match (from, to) {
        (Some(from), Some(to)) => duration((to - from).num_seconds()),
        // Started but not finished: how long it has been going.
        (Some(from), None) => format!("{} so far", duration((Utc::now() - from).num_seconds())),
        _ => "-".to_string(),
    }
}
