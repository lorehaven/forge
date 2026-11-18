//! Times and sizes for showing. Everything is Unix seconds in, text out; no clock and no zone database, so it builds for the
//! browser and for the phone. The caller passes how far its zone is from UTC.

/// `2026-09-29 12:00` for a Unix time, shifted by `offset_minutes` east of UTC.
#[must_use]
pub fn stamp(seconds: i64, offset_minutes: i32) -> String {
    let (year, month, day, hour, minute) = civil(seconds + i64::from(offset_minutes) * 60);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// What a list shows: the time today, `Sep 29` this year, `2025-09-29` before that. `now` is the current Unix time.
#[must_use]
pub fn short(seconds: i64, now: i64, offset_minutes: i32) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let shift = i64::from(offset_minutes) * 60;
    let (year, month, day, hour, minute) = civil(seconds + shift);
    let (now_year, now_month, now_day, _, _) = civil(now + shift);
    if (year, month, day) == (now_year, now_month, now_day) {
        format!("{hour:02}:{minute:02}")
    } else if year == now_year {
        format!("{} {day}", MONTHS[usize::try_from(month - 1).unwrap_or(0)])
    } else {
        format!("{year:04}-{month:02}-{day:02}")
    }
}

/// `1.2 MB`, `340 KB`, `12 B`.
#[must_use]
pub fn size(bytes: u32) -> String {
    const KB: f64 = 1024.0;
    let b = f64::from(bytes);
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{:.1} MB", b / (KB * KB))
    }
}

/// (year, month, day, hour, minute) of a Unix time (Howard Hinnant's algorithm).
fn civil(seconds: i64) -> (i64, i64, i64, i64, i64) {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if month <= 2 { year + 1 } else { year },
        month,
        day,
        rem / 3600,
        rem % 3600 / 60,
    )
}

/// The first words of a text, one line, at most `max` characters, with `…` when cut.
#[must_use]
pub fn snippet(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let mut cut: String = flat.chars().take(max.saturating_sub(1)).collect();
        cut.push('…');
        cut
    }
}

/// Quoted text for a reply: `On <date>, <who> wrote:` and each line behind `> `.
#[must_use]
pub fn quote(text: &str, who: &str, seconds: i64, offset_minutes: i32) -> String {
    let mut out = format!("\n\nOn {}, {who} wrote:\n", stamp(seconds, offset_minutes));
    for line in text.lines() {
        out.push_str("> ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// `Re: x` without stacking, for a subject.
#[must_use]
pub fn reply_subject(subject: &str) -> String {
    prefixed(subject, "Re:")
}

/// `Fwd: x` without stacking.
#[must_use]
pub fn forward_subject(subject: &str) -> String {
    prefixed(subject, "Fwd:")
}

fn prefixed(subject: &str, prefix: &str) -> String {
    let trimmed = subject.trim();
    if trimmed.len() >= prefix.len() && trimmed[..prefix.len()].eq_ignore_ascii_case(prefix) {
        trimmed.to_string()
    } else {
        format!("{prefix} {trimmed}")
    }
}
