//! Cron expressions: when a scheduled pipeline is due.
//!
//! Standard five fields - `minute hour day-of-month month day-of-week` - with `*`, lists (`1,15`),
//! ranges (`1-5`), steps (`*/10`, `0-30/5`), month and weekday names, and the usual `@daily` family.
//! Times are read in a named time zone, so "midnight" stays midnight across daylight-saving changes.
//!
//! Like Vixie cron, when both day-of-month and day-of-week are restricted a day matches if *either*
//! does; when only one is restricted, that one decides.

use quench_core::time::{Civil, Duration, Time, Weekday};
use quench_tz::Zone;
use std::fmt;

/// An expression that is not a valid cron schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronError(String);

impl fmt::Display for CronError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CronError {}

fn err<T>(message: impl Into<String>) -> Result<T, CronError> {
    Err(CronError(message.into()))
}

/// A parsed five-field cron expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    minutes: u64,
    hours: u32,
    days: u32,
    months: u16,
    /// Bit 0 is Sunday.
    weekdays: u8,
    days_restricted: bool,
    weekdays_restricted: bool,
    source: String,
}

const MONTH_NAMES: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const WEEKDAY_NAMES: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

struct Field {
    name: &'static str,
    min: u32,
    max: u32,
    names: &'static [&'static str],
    /// Added to a name's position to get its value (months start at 1).
    name_base: u32,
}

const MINUTE: Field = Field { name: "minute", min: 0, max: 59, names: &[], name_base: 0 };
const HOUR: Field = Field { name: "hour", min: 0, max: 23, names: &[], name_base: 0 };
const DAY: Field = Field { name: "day-of-month", min: 1, max: 31, names: &[], name_base: 0 };
const MONTH: Field = Field { name: "month", min: 1, max: 12, names: &MONTH_NAMES, name_base: 1 };
// 7 is accepted as a second spelling of Sunday and folded onto 0.
const WEEKDAY: Field = Field { name: "day-of-week", min: 0, max: 7, names: &WEEKDAY_NAMES, name_base: 0 };

impl Cron {
    /// Parses an expression.
    ///
    /// # Errors
    ///
    /// [`CronError`] naming the field and value that is wrong.
    pub fn parse(source: &str) -> Result<Self, CronError> {
        let trimmed = source.trim();
        let expanded = match trimmed {
            "@yearly" | "@annually" => "0 0 1 1 *",
            "@monthly" => "0 0 1 * *",
            "@weekly" => "0 0 * * 0",
            "@daily" | "@midnight" => "0 0 * * *",
            "@hourly" => "0 * * * *",
            other if other.starts_with('@') => return err(format!("unknown schedule macro `{other}`")),
            other => other,
        };
        let fields: Vec<&str> = expanded.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = fields.as_slice() else {
            return err(format!(
                "expected 5 fields (minute hour day-of-month month day-of-week), found {}",
                fields.len()
            ));
        };

        let minutes = bits(minute, &MINUTE)?;
        let hours = bits(hour, &HOUR)?;
        let days = bits(day, &DAY)?;
        let months = bits(month, &MONTH)?;
        let mut weekdays = bits(weekday, &WEEKDAY)?;
        // 7 means Sunday.
        if weekdays & (1 << 7) != 0 {
            weekdays = (weekdays & !(1 << 7)) | 1;
        }

        Ok(Self {
            minutes,
            hours: u32::try_from(hours).unwrap_or(0),
            days: u32::try_from(days).unwrap_or(0),
            months: u16::try_from(months).unwrap_or(0),
            weekdays: u8::try_from(weekdays).unwrap_or(0),
            days_restricted: !is_star(day),
            weekdays_restricted: !is_star(weekday),
            source: trimmed.to_string(),
        })
    }

    /// The expression as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// Whether the wall-clock reading `civil` (in the schedule's zone) is a firing minute.
    #[must_use]
    pub fn matches(&self, civil: &Civil) -> bool {
        self.minutes & (1 << civil.minute) != 0
            && self.hours & (1 << civil.hour) != 0
            && self.months & (1 << civil.month) != 0
            && self.day_matches(civil)
    }

    fn day_matches(&self, civil: &Civil) -> bool {
        let of_month = self.days & (1 << civil.day) != 0;
        let weekday = match civil.weekday() {
            Weekday::Sun => 0,
            Weekday::Mon => 1,
            Weekday::Tue => 2,
            Weekday::Wed => 3,
            Weekday::Thu => 4,
            Weekday::Fri => 5,
            Weekday::Sat => 6,
        };
        let of_week = self.weekdays & (1 << weekday) != 0;
        match (self.days_restricted, self.weekdays_restricted) {
            (true, true) => of_month || of_week,
            (true, false) => of_month,
            (false, true) => of_week,
            (false, false) => true,
        }
    }

    /// The first firing strictly after `after`, reading the clock in `zone`; `None` if the
    /// expression never fires (`0 0 31 2 *`, February 31st) within the next eight years.
    ///
    /// A local time that does not exist (the hour clocks skip) does not fire that day; one that
    /// happens twice fires both times, which is why callers key a run on the instant, not the
    /// clock reading.
    #[must_use]
    pub fn next_after(&self, after: Time, zone: &Zone) -> Option<Time> {
        // Strictly after: round down to the minute, then step one.
        let floor = after.timestamp() - after.timestamp().rem_euclid(60);
        let mut t = Time::from_unix(floor + 60);
        let horizon = floor + 8 * 366 * 86_400;

        let mut steps = 0u32;
        while t.timestamp() <= horizon {
            steps += 1;
            if steps > 200_000 {
                return None;
            }
            let civil = zone.civil_at(t);
            let seconds_into_hour = i64::from(civil.minute) * 60 + i64::from(civil.second);
            let seconds_into_day = i64::from(civil.hour) * 3_600 + seconds_into_hour;

            if self.months & (1 << civil.month) == 0 || !self.day_matches(&civil) {
                // Wrong day: skip to just before the next local midnight (an hour early, so a
                // daylight-saving change inside the day can never make us step over a match).
                let skip = (86_400 - seconds_into_day - 3_600).max(3_600);
                t += Duration::seconds(skip);
            } else if self.hours & (1 << civil.hour) == 0 {
                t += Duration::seconds(3_600 - seconds_into_hour);
            } else if self.minutes & (1 << civil.minute) == 0 {
                t += Duration::seconds(60 - i64::from(civil.second));
            } else {
                return Some(t);
            }
        }
        None
    }
}

impl fmt::Display for Cron {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}

fn is_star(field: &str) -> bool {
    field.starts_with('*')
}

/// The set of values a field allows, as a bit set (bit `n` = value `n`).
fn bits(field: &str, spec: &Field) -> Result<u64, CronError> {
    let mut set = 0u64;
    for part in field.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => {
                let Ok(step) = step.parse::<u32>() else {
                    return err(format!("{}: `{step}` is not a valid step", spec.name));
                };
                if step == 0 {
                    return err(format!("{}: a step of 0 never advances", spec.name));
                }
                (range, step)
            }
            None => (part, 1),
        };
        let (low, high) = if range == "*" {
            (spec.min, spec.max)
        } else if let Some((low, high)) = range.split_once('-') {
            (value(low, spec)?, value(high, spec)?)
        } else {
            let single = value(range, spec)?;
            // `5/10` means "from 5, every 10", up to the field's maximum.
            (single, if part.contains('/') { spec.max } else { single })
        };
        if low > high {
            return err(format!("{}: range `{range}` runs backwards", spec.name));
        }
        let mut v = low;
        while v <= high {
            set |= 1 << v;
            v += step;
        }
    }
    if set == 0 {
        return err(format!("{}: `{field}` allows no value", spec.name));
    }
    Ok(set)
}

fn value(text: &str, spec: &Field) -> Result<u32, CronError> {
    let lower = text.to_ascii_lowercase();
    let parsed = lower.parse::<u32>().ok().or_else(|| {
        spec.names
            .iter()
            .position(|name| *name == lower)
            .and_then(|i| u32::try_from(i).ok())
            .map(|i| i + spec.name_base)
    });
    match parsed {
        Some(v) if (spec.min..=spec.max).contains(&v) => Ok(v),
        Some(v) => err(format!("{}: {v} is outside {}-{}", spec.name, spec.min, spec.max)),
        None => err(format!("{}: `{text}` is not a number or name", spec.name)),
    }
}
