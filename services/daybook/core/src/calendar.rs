//! Due dates as the person sees them: which day, how urgent, how written.

use quench_core::date::Date;
use quench_core::time::{Civil, Duration, Time};
use quench_tz::Zone;

use crate::model::Task;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// 1 = Monday to 7 = Sunday.
#[must_use]
pub const fn iso_weekday(date: Date) -> u8 {
    // 1970-01-01 was a Thursday.
    (date.days_since_epoch() + 3).rem_euclid(7) as u8 + 1
}

/// The date `now` is on in `zone`.
#[must_use]
pub fn today(now: Time, zone: &Zone) -> Date {
    zone.civil_at(now).date()
}

/// The day a due date is on: an all-day one is the UTC date it was stored with, in any zone; a timed one is the day in `zone`.
#[must_use]
pub fn due_day(due: Time, all_day: bool, zone: &Zone) -> Date {
    if all_day {
        due.civil().date()
    } else {
        zone.civil_at(due).date()
    }
}

/// The stored value of an all-day date: midnight UTC.
#[must_use]
pub fn all_day_time(date: Date) -> Time {
    date.at(0, 0, 0).to_time().unwrap_or_default()
}

/// The instant of a wall-clock reading in `zone`. In the hour that does not exist the reading is moved on by an hour,
/// in the hour that happens twice it is the first.
#[must_use]
pub fn at_local(date: Date, hour: u32, minute: u32, zone: &Zone) -> Option<Time> {
    let civil: Civil = date.at(hour, minute, 0);
    match zone.resolve(&civil) {
        quench_tz::Resolved::Gap => zone
            .resolve(
                &date
                    .at(hour, minute, 0)
                    .to_time()
                    .map(|t| t + Duration::hours(1))?
                    .civil(),
            )
            .earliest(),
        other => other.earliest(),
    }
}

/// Hour and minute of a timed due date in `zone`.
#[must_use]
pub fn clock_of(due: Time, zone: &Zone) -> (u32, u32) {
    let c = zone.civil_at(due);
    (c.hour, c.minute)
}

/// "09:05".
#[must_use]
pub fn format_clock(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}")
}

/// Whether the task is late: a timed one when its time has passed, an all-day one once its day has.
#[must_use]
pub fn is_overdue(due: Time, all_day: bool, now: Time, zone: &Zone) -> bool {
    if all_day {
        due_day(due, true, zone) < today(now, zone)
    } else {
        due < now
    }
}

/// Where a task falls among others: the day, then the minute (an all-day task comes before the timed ones of its day).
#[must_use]
pub fn due_sort_key(due: Time, all_day: bool, zone: &Zone) -> (i64, i32) {
    let day = due_day(due, all_day, zone).days_since_epoch();
    if all_day {
        (day, -1)
    } else {
        let (h, m) = clock_of(due, zone);
        (day, i32::try_from(h * 60 + m).unwrap_or(0))
    }
}

/// How soon a task is due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Overdue,
    Today,
    Tomorrow,
    /// Within three days.
    Soon,
    /// Within a week.
    Week,
    Later,
}

impl Urgency {
    /// The colour of the due text; `None` is the ordinary text colour. Overdue is the theme's error colour in the app, this is
    /// the red the widget uses.
    #[must_use]
    pub const fn rgb(self) -> Option<(u8, u8, u8)> {
        match self {
            Self::Overdue => Some((0xF8, 0x71, 0x71)),
            Self::Today => Some((0x4A, 0xDE, 0x80)),
            Self::Tomorrow => Some((0xFB, 0x92, 0x3C)),
            Self::Soon => Some((0xFA, 0xCC, 0x15)),
            Self::Week => Some((0x60, 0xA5, 0xFA)),
            Self::Later => None,
        }
    }
}

#[must_use]
pub fn urgency(due: Time, all_day: bool, now: Time, zone: &Zone) -> Urgency {
    if is_overdue(due, all_day, now, zone) {
        return Urgency::Overdue;
    }
    match due_day(due, all_day, zone).days_since(today(now, zone)) {
        i64::MIN..=0 => Urgency::Today,
        1 => Urgency::Tomorrow,
        2..=3 => Urgency::Soon,
        4..=7 => Urgency::Week,
        _ => Urgency::Later,
    }
}

/// The urgency of an open task with a due date.
#[must_use]
pub fn task_urgency(task: &Task, now: Time, zone: &Zone) -> Option<Urgency> {
    if task.is_completed {
        return None;
    }
    task.due_at
        .map(|due| urgency(due, task.due_all_day, now, zone))
}

/// "Mon 12 Oct" or, outside this year, "Mon 12 Oct 2027".
#[must_use]
pub fn format_date(date: Date, today: Date) -> String {
    let (year, month, day) = date.ymd();
    let name = WEEKDAYS[usize::from(iso_weekday(date) - 1)];
    let month = MONTHS[(month as usize).saturating_sub(1) % 12];
    if year == today.year() {
        format!("{name} {day} {month}")
    } else {
        format!("{name} {day} {month} {year}")
    }
}

/// "Today 14:30", "Tomorrow", "Yesterday", "Mon 12 Oct 14:30". An all-day date has no time.
#[must_use]
pub fn format_due(due: Time, all_day: bool, now: Time, zone: &Zone) -> String {
    let date = due_day(due, all_day, zone);
    let today = today(now, zone);
    let day = match date.days_since(today) {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        _ => format_date(date, today),
    };
    if all_day {
        day
    } else {
        let (h, m) = clock_of(due, zone);
        format!("{day} {}", format_clock(h, m))
    }
}
