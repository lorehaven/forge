//! Repeating tasks: the rule, its validation, and the date arithmetic that finds the next occurrence.
//!
//! Everything here is pure - no database, no clock (callers pass `now`) - so the arithmetic, which is
//! where the awkward cases live (clock changes, short months, late completion), is tested directly.

use quench_core::date::Date;
use quench_core::time::{Civil, Duration, Time};
use quench_serde::{Deserialize, Serialize};
use quench_tz::{Resolved, Zone};

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
        if Zone::named(&self.timezone).is_err() {
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

    fn zone(&self) -> Zone {
        Zone::named(&self.timezone).unwrap_or_else(|_| Zone::utc())
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
    pub fn next_due(&self, anchor: Time, due: Time, now: Time) -> Option<Time> {
        let zone = self.zone();
        let anchor_local = zone.civil_at(anchor);
        let anchor_date = anchor_local.date();

        match self.mode {
            Mode::AfterCompletion => {
                let today = zone.civil_at(now).date();
                let date = self.add_units(today, 1)?;
                Some(local_to_utc(&zone, &with_time_of(date, &anchor_local)))
            }
            Mode::Fixed => {
                let after = due.max(now);
                self.occurrence_dates(anchor_date)
                    .map(|date| local_to_utc(&zone, &with_time_of(date, &anchor_local)))
                    .find(|at| *at > after)
            }
        }
    }

    /// [`next_due`](Self::next_due) for an all-day task, whose due date is a calendar date with no time.
    ///
    /// Such a date is stored as midnight UTC and read back as that UTC date, whatever the viewer's zone, so
    /// the arithmetic is on that date. Only "today" is zone-dependent, and it is today in the rule's zone.
    /// Returns midnight UTC of the next date.
    #[must_use]
    pub fn next_due_all_day(&self, anchor: Time, due: Time, now: Time) -> Option<Time> {
        let today = self.zone().civil_at(now).date();
        let date = match self.mode {
            Mode::AfterCompletion => self.add_units(today, 1)?,
            Mode::Fixed => {
                let threshold = due.civil().date().max(today);
                self.occurrence_dates(anchor.civil().date())
                    .find(|date| *date > threshold)?
            }
        };
        date.at(0, 0, 0).to_time()
    }

    /// Where the reminder goes when the due date moves from `due` to `next`: the same wall-clock distance
    /// before it, in the rule's zone. Measured on the wall clock rather than in elapsed time so "a day
    /// before at 09:00" stays at 09:00 across a clock change. For an all-day task the due moment is
    /// midnight of its date.
    #[must_use]
    pub fn shift_reminder(&self, remind: Time, due: Time, next: Time, all_day: bool) -> Time {
        let zone = self.zone();
        // A wall-clock reading as an instant-shaped number, so readings can be subtracted.
        let wall = |at: Time| -> Time {
            let civil = if all_day {
                at.civil().date().at(0, 0, 0)
            } else {
                zone.civil_at(at)
            };
            civil.to_time().unwrap_or(at)
        };
        // The reminder is a moment on the clock in every case; only the due moment of an all-day task is a date.
        let remind_wall = zone.civil_at(remind).to_time().unwrap_or(remind);
        let lead = wall(due) - remind_wall;
        let shifted = wall(next) - lead;
        local_to_utc(&zone, &shifted.civil())
    }

    /// `date` moved forward by `steps` times the rule's interval. Months and years are added to the
    /// date itself (clamping to the month's last day), not by repeating a smaller step.
    fn add_units(&self, date: Date, steps: u32) -> Option<Date> {
        let total = u64::from(self.interval) * u64::from(steps);
        match self.unit {
            Unit::Day => date.add_days(i64::try_from(total).ok()?),
            Unit::Week => date.add_days(i64::try_from(total.checked_mul(7)?).ok()?),
            Unit::Month => date.add_months(i32::try_from(total).ok()?),
            Unit::Year => date.add_months(i32::try_from(total * 12).ok()?),
        }
    }

    /// Every date of a fixed schedule counted from `anchor_date`, in order, ending after [`MAX_STEPS`]
    /// occurrences. With weekdays: every `interval`-th week (Monday-based, counted from the anchor's
    /// week), on each listed weekday that is not before the anchor.
    fn occurrence_dates(&self, anchor_date: Date) -> Box<dyn Iterator<Item = Date> + '_> {
        if self.weekdays.is_empty() {
            return Box::new((0..MAX_STEPS).map_while(move |k| self.add_units(anchor_date, k)));
        }
        // 1970-01-01 was a Thursday, so day 0 is 3 days after a Monday.
        let days_from_monday = (anchor_date.days_since_epoch() + 3).rem_euclid(7);
        let monday = anchor_date.add_days(-days_from_monday);
        Box::new(
            (0..MAX_STEPS / 4)
                .map_while(move |week| self.add_units(monday?, week))
                .flat_map(move |start| {
                    self.weekdays
                        .iter()
                        .filter_map(move |day| start.add_days(i64::from(*day) - 1))
                        .filter(move |date| *date >= anchor_date)
                }),
        )
    }
}

/// `date` at the time of day `time` shows.
fn with_time_of(date: Date, time: &Civil) -> Civil {
    Civil {
        nanosecond: time.nanosecond,
        ..date.at(time.hour, time.minute, time.second)
    }
}

/// A local wall-clock time as a UTC instant. A time that happens twice (clocks going back) is the
/// first; one that does not exist (clocks going forward) lands an hour later, where it would have been.
fn local_to_utc(zone: &Zone, local: &Civil) -> Time {
    match zone.resolve(local) {
        Resolved::Single(at) | Resolved::Ambiguous { earlier: at, .. } => at,
        Resolved::Gap => {
            let as_utc = local.to_time().unwrap_or_default();
            zone.resolve(&(as_utc + Duration::hours(1)).civil())
                .earliest()
                .unwrap_or(as_utc)
        }
    }
}
