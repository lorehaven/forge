//! Dates as mail writes them (RFC 2822), as Unix seconds.

/// `Tue, 29 Sep 2026 12:00:00 +0200` and its sloppy relatives: no weekday, no seconds, two-digit years, zone names
/// (`GMT`, `EST`, ...), a trailing `(CEST)` comment. `None` when no date can be read.
#[must_use]
pub fn parse(value: &str) -> Option<i64> {
    let value = strip_comment(value);
    let value = value.trim();
    let value = value.split_once(',').map_or(value, |(_, rest)| rest).trim();
    let mut words = value.split_whitespace();
    let day: i64 = words.next()?.parse().ok()?;
    let month = month_number(words.next()?)?;
    let mut year: i64 = words.next()?.parse().ok()?;
    if year < 50 {
        year += 2000;
    } else if year < 100 {
        year += 1900;
    }
    let time = words.next().unwrap_or("00:00:00");
    let mut parts = time.split(':');
    let hour: i64 = parts.next()?.parse().ok()?;
    let minute: i64 = parts.next().unwrap_or("0").parse().ok()?;
    let second: i64 = parts.next().unwrap_or("0").parse().ok()?;
    let zone = words.next().map_or(0, zone_offset);
    if !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - zone)
}

fn strip_comment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut depth = 0usize;
    for c in value.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn month_number(name: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let name = name.get(..3)?.to_ascii_lowercase();
    MONTHS.iter().position(|m| *m == name).map(|i| i as i64 + 1)
}

/// The zone as seconds east of UTC.
fn zone_offset(zone: &str) -> i64 {
    if let Some(digits) = zone.strip_prefix(['+', '-'])
        && digits.len() == 4
        && let Ok(n) = digits.parse::<i64>()
    {
        let seconds = (n / 100) * 3600 + (n % 100) * 60;
        return if zone.starts_with('-') {
            -seconds
        } else {
            seconds
        };
    }
    let hours = match zone.to_ascii_uppercase().as_str() {
        "EDT" => -4,
        "EST" | "CDT" => -5,
        "CST" | "MDT" => -6,
        "MST" | "PDT" => -7,
        "PST" => -8,
        "CET" => 1,
        "CEST" => 2,
        _ => 0,
    };
    hours * 3600
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
#[must_use]
pub const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// RFC 2822 text for a Unix time, in UTC (`Tue, 29 Sep 2026 12:00:00 +0000`).
#[must_use]
pub fn format(seconds: i64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{}, {day} {} {year:04} {:02}:{:02}:{:02} +0000",
        DAYS[usize::try_from(days.rem_euclid(7)).unwrap_or(0)],
        MONTHS[usize::try_from(month - 1).unwrap_or(0)],
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

const fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
