//! Repeating tasks: the rule, its validation, and the date arithmetic that finds the next occurrence.
//!
//! Everything here is pure - no database, no clock (callers pass `now`) - so the arithmetic, which is
//! where the awkward cases live (clock changes, short months, late completion), is tested directly.

use chrono::{DateTime, Days, Duration, LocalResult, Months, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// The most `interval` a rule may have. Far beyond any real use; it keeps the arithmetic in range.
pub const MAX_INTERVAL: u32 = 999;

/// How many occurrences are tried before giving up - about 55 years of a daily task. Reaching it means
/// the rule's schedule is far in the past; the task is then simply completed rather than rolled.
const MAX_STEPS: u32 = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Day,
    Week,
    Month,
    Year,
}

impl Unit {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            "year" => Some(Self::Year),
            _ => None,
        }
    }
}

/// What the next occurrence is counted from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The schedule: a Monday task stays on Mondays however late it is finished.
    #[default]
    Fixed,
    /// The day it is finished: "3 days after the last time".
    AfterCompletion,
}

impl Mode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::AfterCompletion => "after_completion",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "fixed" => Some(Self::Fixed),
            "after_completion" => Some(Self::AfterCompletion),
            _ => None,
        }
    }
}

fn utc() -> String {
    "UTC".to_string()
}

/// "Every `interval` `unit`s", optionally on given weekdays, counted per `mode` in `timezone`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recurrence {
    pub unit: Unit,
    pub interval: u32,
    /// ISO weekdays, 1 = Monday ... 7 = Sunday. Only for a fixed weekly rule; empty means "the weekday the
    /// task is due on".
    #[serde(default)]
    pub weekdays: Vec<u8>,
    #[serde(default)]
    pub mode: Mode,
    /// IANA name. Evaluating in the user's zone is what keeps a 09:00 task at 09:00 across a clock change.
    #[serde(default = "utc")]
    pub timezone: String,
}

impl Recurrence {
    /// Checks the rule and returns it with `weekdays` sorted and de-duplicated; `Err` is the 400 message.
    pub fn validated(mut self) -> Result<Self, &'static str> {
        if self.interval == 0 || self.interval > MAX_INTERVAL {
            return Err("recurrence interval must be between 1 and 999");
        }
        if self.timezone.parse::<Tz>().is_err() {
            return Err("recurrence timezone is not a known IANA zone");
        }
        self.weekdays.sort_unstable();
        self.weekdays.dedup();
        if !self.weekdays.is_empty() {
            if self.unit != Unit::Week {
                return Err("recurrence weekdays only apply to a weekly rule");
            }
            if self.mode != Mode::Fixed {
                return Err("recurrence weekdays need the fixed schedule, not after_completion");
            }
            if self.weekdays.iter().any(|day| !(1..=7).contains(day)) {
                return Err("recurrence weekdays are 1 (Monday) to 7 (Sunday)");
            }
        }
        Ok(self)
    }

    fn zone(&self) -> Tz {
        self.timezone.parse().unwrap_or(Tz::UTC)
    }

    /// The due date after `due` is completed at `now`, or `None` if no later occurrence can be found.
    ///
    /// `anchor` is the date the schedule is counted from (see the migration): occurrences are
    /// `anchor + k * interval units`, each computed from the anchor itself so a short month cannot
    /// drag later ones off their day.
    ///
    /// * **Fixed** - the first occurrence after both `due` and `now`: finishing early moves to the next
    ///   one rather than the one you just did; finishing late skips occurrences that are already past.
    /// * **After completion** - `interval` units after today (in the rule's zone), at the anchor's time.
    #[must_use]
    pub fn next_due(
        &self,
        anchor: DateTime<Utc>,
        due: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Option<DateTime<Utc>> {
        let zone = self.zone();
        let anchor_local = anchor.with_timezone(&zone);
        let (anchor_date, time) = (anchor_local.date_naive(), anchor_local.time());

        match self.mode {
            Mode::AfterCompletion => {
                let today = now.with_timezone(&zone).date_naive();
                let date = self.add_units(today, 1)?;
                Some(local_to_utc(zone, date, time))
            }
            Mode::Fixed => {
                let after = due.max(now);
                if self.weekdays.is_empty() {
                    (0..MAX_STEPS).find_map(|k| {
                        let date = self.add_units(anchor_date, k)?;
                        Some(local_to_utc(zone, date, time)).filter(|at| *at > after)
                    })
                } else {
                    self.next_on_weekdays(zone, anchor_date, time, after)
                }
            }
        }
    }

    /// `date` moved forward by `steps` times the rule's interval. Months and years are added to the
    /// date itself (clamping to the month's last day), not by repeating a smaller step.
    fn add_units(&self, date: NaiveDate, steps: u32) -> Option<NaiveDate> {
        let total = u64::from(self.interval) * u64::from(steps);
        match self.unit {
            Unit::Day => date.checked_add_days(Days::new(total)),
            Unit::Week => date.checked_add_days(Days::new(total.checked_mul(7)?)),
            Unit::Month => date.checked_add_months(Months::new(u32::try_from(total).ok()?)),
            Unit::Year => date.checked_add_months(Months::new(u32::try_from(total * 12).ok()?)),
        }
    }

    /// Fixed weekly rule on specific weekdays: every `interval`-th week, counted from the anchor's week
    /// (Monday-based), on each listed weekday that is not before the anchor.
    fn next_on_weekdays(
        &self,
        zone: Tz,
        anchor_date: NaiveDate,
        time: NaiveTime,
        after: DateTime<Utc>,
    ) -> Option<DateTime<Utc>> {
        let monday = anchor_date.checked_sub_days(Days::new(u64::from(
            chrono::Datelike::weekday(&anchor_date).num_days_from_monday(),
        )))?;
        (0..MAX_STEPS / 4).find_map(|week| {
            let start = self.add_units(monday, week)?;
            self.weekdays.iter().find_map(|day| {
                let date = start.checked_add_days(Days::new(u64::from(*day - 1)))?;
                if date < anchor_date {
                    return None;
                }
                Some(local_to_utc(zone, date, time)).filter(|at| *at > after)
            })
        })
    }
}

/// A local wall-clock time as a UTC instant. A time that happens twice (clocks going back) is the
/// first; one that does not exist (clocks going forward) lands an hour later, where it would have been.
fn local_to_utc(zone: Tz, date: NaiveDate, time: NaiveTime) -> DateTime<Utc> {
    let naive = date.and_time(time);
    match zone.from_local_datetime(&naive) {
        LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => at.with_timezone(&Utc),
        LocalResult::None => zone
            .from_local_datetime(&(naive + Duration::hours(1)))
            .earliest()
            .map_or_else(|| naive.and_utc(), |at| at.with_timezone(&Utc)),
    }
}
