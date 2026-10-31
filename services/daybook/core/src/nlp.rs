//! Due dates written in the title ("Pay rent nov 6 5pm", "call mum tomorrow at 9", "review in 2 weeks"), as Todoist reads
//! them: the words that name a day or a time are found, turned into a [`Due`] and taken out of the title.
//!
//! English only. What is understood, in any order and with an optional "on", "at", "by" or "@" in front:
//! - days: `today`, `tomorrow`/`tmr`, `tonight`, `monday`..`sunday` (and `mon`, `tue`, ...), `next <weekday>`,
//!   `next week`, `next month`, `weekend`;
//! - dates: `nov 6`, `november 6th`, `6 nov`, `6th of november`, any of them with a year, and `2026-11-06`;
//! - offsets: `in 3 days`, `in 2 weeks`, `in a month`, `in 90 minutes`, `in 2 hours`;
//! - times: `5pm`, `5:30 pm`, `17:00`, `noon`, `midnight`, `at 5` (one to six is taken as the afternoon).
//!
//! A day without a time is all day; a time without a day is today, or tomorrow when it has passed.

use quench_core::date::Date;
use quench_core::time::{Duration, Time};
use quench_tz::Zone;

use crate::calendar::{iso_weekday, today};
use crate::draft::Due;

/// What was found in a title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub due: Due,
    /// The title without the words that were read as the date.
    pub content: String,
    /// Those words as they were written, for showing what was understood.
    pub phrase: String,
}

const MONTHS: [(&str, u32); 23] = [
    ("january", 1),
    ("jan", 1),
    ("february", 2),
    ("feb", 2),
    ("march", 3),
    ("mar", 3),
    ("april", 4),
    ("apr", 4),
    ("may", 5),
    ("june", 6),
    ("jun", 6),
    ("july", 7),
    ("jul", 7),
    ("august", 8),
    ("aug", 8),
    ("september", 9),
    ("sept", 9),
    ("sep", 9),
    ("october", 10),
    ("oct", 10),
    ("november", 11),
    ("nov", 11),
    ("december", 12),
];

const WEEKDAYS: [(&str, u8); 17] = [
    ("monday", 1),
    ("mon", 1),
    ("tuesday", 2),
    ("tues", 2),
    ("tue", 2),
    ("wednesday", 3),
    ("wed", 3),
    ("thursday", 4),
    ("thurs", 4),
    ("thur", 4),
    ("thu", 4),
    ("friday", 5),
    ("fri", 5),
    ("saturday", 6),
    ("sat", 6),
    ("sunday", 7),
    ("sun", 7),
];

/// A word of the title: the letters and digits of it (no trailing `,.;!?`) and where it stands.
struct Word {
    lower: String,
    start: usize,
    /// Where the word ends, with the punctuation after it.
    end: usize,
}

fn words(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                start = None;
                let raw = &text[s..i];
                let trimmed = raw.trim_end_matches([',', '.', ';', '!', '?']);
                if !trimmed.is_empty() {
                    out.push(Word {
                        lower: trimmed.to_lowercase(),
                        start: s,
                        end: i,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn month_of(word: &str) -> Option<u32> {
    MONTHS.iter().find(|(n, _)| *n == word).map(|(_, m)| *m)
}

fn weekday_of(word: &str) -> Option<u8> {
    WEEKDAYS.iter().find(|(n, _)| *n == word).map(|(_, d)| *d)
}

/// "6" or "6th" as a day of the month.
fn day_number(word: &str) -> Option<u32> {
    let digits = word
        .strip_suffix("st")
        .or_else(|| word.strip_suffix("nd"))
        .or_else(|| word.strip_suffix("rd"))
        .or_else(|| word.strip_suffix("th"))
        .unwrap_or(word);
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|d| (1..=31).contains(d))
}

fn year_number(word: &str) -> Option<i32> {
    (word.len() == 4 && word.starts_with("20") && word.bytes().all(|b| b.is_ascii_digit()))
        .then(|| word.parse().ok())
        .flatten()
}

/// The next time `month`/`day` comes round (today counts), or the given year's.
fn month_day(month: u32, day: u32, year: Option<i32>, today: Date) -> Option<Date> {
    if let Some(year) = year {
        return Date::from_ymd(year, month, day);
    }
    let this = Date::from_ymd(today.year(), month, day)?;
    if this.days_since(today) >= 0 {
        Some(this)
    } else {
        Date::from_ymd(today.year() + 1, month, day)
    }
}

/// The first `weekday` after `today`.
fn upcoming(weekday: u8, today: Date) -> Option<Date> {
    let ahead = (i64::from(weekday) - i64::from(iso_weekday(today))).rem_euclid(7);
    today.add_days(if ahead == 0 { 7 } else { ahead })
}

/// The `weekday` of next week (the week that starts on the Monday after `today`).
fn next_week_day(weekday: u8, today: Date) -> Option<Date> {
    let monday = today.add_days(8 - i64::from(iso_weekday(today)))?;
    monday.add_days(i64::from(weekday) - 1)
}

/// A time of day written as `5pm`, `5:30pm`, `17:00`, `noon` or `midnight`; the number of words it used. A bare hour
/// (`at 5`) is read only after "at", `after_at`.
fn clock_at(ws: &[Word], i: usize, after_at: bool) -> Option<((u32, u32), usize)> {
    let w = ws[i].lower.as_str();
    match w {
        "noon" | "midday" => return Some(((12, 0), 1)),
        "midnight" => return Some(((0, 0), 1)),
        _ => {}
    }
    let (digits, suffix) = w
        .find(|c: char| !(c.is_ascii_digit() || c == ':'))
        .map_or((w, ""), |p| (&w[..p], &w[p..]));
    if digits.is_empty() {
        return None;
    }
    let (hour, minute, colon) = match digits.split_once(':') {
        Some((h, m)) if h.len() <= 2 && m.len() == 2 => {
            (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?, true)
        }
        Some(_) => return None,
        None if digits.len() <= 2 => (digits.parse::<u32>().ok()?, 0, false),
        None => return None,
    };
    let meridiem = match suffix {
        "am" | "a.m." | "a.m" => Some(false),
        "pm" | "p.m." | "p.m" => Some(true),
        "" => None,
        _ => return None,
    };
    // "5 pm": the meridiem is the next word.
    let (meridiem, used) = match (meridiem, ws.get(i + 1).map(|n| n.lower.as_str())) {
        (None, Some("am" | "a.m." | "a.m")) => (Some(false), 2),
        (None, Some("pm" | "p.m." | "p.m")) => (Some(true), 2),
        (m, _) => (m, 1),
    };
    if minute > 59 {
        return None;
    }
    match meridiem {
        Some(pm) => {
            if !(1..=12).contains(&hour) {
                return None;
            }
            Some((((hour % 12) + if pm { 12 } else { 0 }, minute), used))
        }
        None if colon => (hour < 24).then_some(((hour, minute), used)),
        // A bare hour: only after "at", and one to six means the afternoon.
        None if after_at && (1..=12).contains(&hour) => {
            Some((((if hour <= 6 { hour + 12 } else { hour % 24 }), 0), used))
        }
        None => None,
    }
}

/// A day, from the word at `i`: the date and how many words it used.
fn day_at(ws: &[Word], i: usize, now_date: Date) -> Option<(Date, usize, bool)> {
    let w = ws[i].lower.as_str();
    let next = ws.get(i + 1).map(|n| n.lower.as_str());
    match w {
        "today" => return Some((now_date, 1, false)),
        "tonight" => return Some((now_date, 1, true)),
        "tomorrow" | "tmr" | "tmrw" | "tom" => return now_date.add_days(1).map(|d| (d, 1, false)),
        "weekend" => return upcoming(6, now_date).map(|d| (d, 1, false)),
        "next" => {
            match next {
                Some("week") => return next_week_day(1, now_date).map(|d| (d, 2, false)),
                Some("month") => return now_date.add_months(1).map(|d| (d, 2, false)),
                Some("weekend") => return next_week_day(6, now_date).map(|d| (d, 2, false)),
                Some(n) => {
                    if let Some(day) = weekday_of(n) {
                        return next_week_day(day, now_date).map(|d| (d, 2, false));
                    }
                }
                None => {}
            }
            return None;
        }
        "this" => {
            if let Some(n) = next {
                if n == "weekend" {
                    return upcoming(6, now_date).map(|d| (d, 2, false));
                }
                if let Some(day) = weekday_of(n) {
                    return upcoming(day, now_date).map(|d| (d, 2, false));
                }
            }
            return None;
        }
        "in" => return offset(ws, i, now_date),
        _ => {}
    }
    if let Some(day) = weekday_of(w) {
        return upcoming(day, now_date).map(|d| (d, 1, false));
    }
    // 2026-11-06.
    if let [y, m, d] = w.split('-').collect::<Vec<_>>()[..]
        && y.len() == 4
        && let (Ok(y), Ok(m), Ok(d)) = (y.parse(), m.parse(), d.parse())
    {
        return Date::from_ymd(y, m, d).map(|d| (d, 1, false));
    }
    // "nov 6", "november 6th 2027".
    if let Some(month) = month_of(w) {
        let day = ws.get(i + 1).and_then(|n| day_number(&n.lower))?;
        let year = ws.get(i + 2).and_then(|n| year_number(&n.lower));
        let used = 2 + usize::from(year.is_some());
        return month_day(month, day, year, now_date).map(|d| (d, used, false));
    }
    // "6 nov", "6th of november 2027".
    if let Some(day) = day_number(w) {
        let (month_at, skipped) = if next == Some("of") {
            (i + 2, 1)
        } else {
            (i + 1, 0)
        };
        let month = ws.get(month_at).and_then(|n| month_of(&n.lower))?;
        let year = ws.get(month_at + 1).and_then(|n| year_number(&n.lower));
        let used = 2 + skipped + usize::from(year.is_some());
        return month_day(month, day, year, now_date).map(|d| (d, used, false));
    }
    None
}

/// "in 3 days", "in a week" (days, weeks and months; minutes and hours are read by [`parse_due`]).
fn offset(ws: &[Word], i: usize, now_date: Date) -> Option<(Date, usize, bool)> {
    let count = ws.get(i + 1)?.lower.as_str();
    let n: i64 = if matches!(count, "a" | "an") {
        1
    } else {
        count.parse().ok().filter(|n| (1..=1000).contains(n))?
    };
    let unit = ws.get(i + 2)?.lower.as_str();
    let date = match unit {
        "day" | "days" => now_date.add_days(n)?,
        "week" | "weeks" => now_date.add_days(7 * n)?,
        "month" | "months" => now_date.add_months(i32::try_from(n).ok()?)?,
        _ => return None,
    };
    Some((date, 3, false))
}

/// "in 90 minutes", "in 2 hours": the moment, and how many words.
fn short_offset(ws: &[Word], i: usize, now: Time) -> Option<(Time, usize)> {
    if ws[i].lower != "in" {
        return None;
    }
    let count = ws.get(i + 1)?.lower.as_str();
    let n: i64 = if matches!(count, "a" | "an") {
        1
    } else {
        count.parse().ok().filter(|n| (1..=10_000).contains(n))?
    };
    let by = match ws.get(i + 2)?.lower.as_str() {
        "minute" | "minutes" | "min" | "mins" => Duration::minutes(n),
        "hour" | "hours" | "hr" | "hrs" => Duration::hours(n),
        _ => return None,
    };
    Some((now + by, 3))
}

/// The date and time written in `title`, if any, as of `now` in `zone`. Nothing is found when the title would be
/// left empty, or when the words are not a date ("buy 5 apples").
#[must_use]
pub fn parse_due(title: &str, now: Time, zone: &Zone) -> Option<Parsed> {
    let ws = words(title);
    let now_date = today(now, zone);
    let mut day: Option<(Date, bool)> = None;
    let mut clock: Option<(u32, u32)> = None;
    // Byte ranges to cut out.
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < ws.len() {
        let after_at = i > 0 && matches!(ws[i - 1].lower.as_str(), "at" | "@");
        let mut taken = None;
        if day.is_none()
            && clock.is_none()
            && let Some((at, used)) = short_offset(&ws, i, now)
        {
            let civil = zone.civil_at(at);
            day = Some((civil.date(), false));
            clock = Some((civil.hour, civil.minute));
            taken = Some(used);
        }
        if taken.is_none()
            && day.is_none()
            && let Some((date, used, tonight)) = day_at(&ws, i, now_date)
        {
            day = Some((date, tonight));
            taken = Some(used);
        }
        if taken.is_none()
            && clock.is_none()
            && let Some((c, used)) = clock_at(&ws, i, after_at)
        {
            clock = Some(c);
            taken = Some(used);
        }
        let Some(used) = taken else {
            i += 1;
            continue;
        };
        // A joining word right before it goes too: "on nov 6", "at 5pm", "by friday", "@5pm".
        let mut first = i;
        if i > 0
            && matches!(
                ws[i - 1].lower.as_str(),
                "on" | "at" | "by" | "@" | "due" | "before"
            )
        {
            first = i - 1;
        }
        cuts.push((ws[first].start, ws[i + used - 1].end));
        i += used;
    }
    let (date, tonight) = match day {
        Some(day) => day,
        None => {
            // Only a time: today, or tomorrow when it has gone.
            let (h, m) = clock?;
            let c = zone.civil_at(now);
            let later = (h, m) > (c.hour, c.minute);
            (
                if later {
                    now_date
                } else {
                    now_date.add_days(1)?
                },
                false,
            )
        }
    };
    let clock = clock.or(tonight.then_some((20, 0)));
    // The title without the cut words, the spaces closed up.
    let mut content = String::new();
    let mut at = 0;
    for &(from, to) in &cuts {
        content.push_str(&title[at..from]);
        at = to;
    }
    content.push_str(&title[at..]);
    let content = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let content = content.trim_matches([',', ';', '-']).trim().to_string();
    if content.is_empty() {
        return None;
    }
    let phrase = cuts
        .iter()
        .map(|&(from, to)| title[from..to].trim_end_matches([',', '.', ';', '!', '?']))
        .collect::<Vec<_>>()
        .join(" ");
    Some(Parsed {
        due: Due { date, clock },
        content,
        phrase,
    })
}
