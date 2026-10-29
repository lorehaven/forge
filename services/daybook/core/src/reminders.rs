//! Reminders: the presets of the task form and the notifications to schedule.

use quench_core::date::Date;
use quench_core::time::{Duration, Time};
use quench_tz::Zone;

use crate::calendar::{at_local, due_day, format_date, today};
use crate::model::Task;

/// The most notifications scheduled at once (Android keeps a limited number of alarms).
pub const MAX_SCHEDULED: usize = 200;

/// The hour an "on the day" reminder of an all-day task fires.
pub const DAY_REMINDER_HOUR: u32 = 9;

/// A reminder relative to the due date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    AtDue,
    TenMinutes,
    OneHour,
    OneDay,
    OnTheDay,
    DayBefore,
    TwoDaysBefore,
}

impl Preset {
    pub const TIMED: [Self; 4] = [Self::AtDue, Self::TenMinutes, Self::OneHour, Self::OneDay];
    pub const ALL_DAY: [Self; 3] = [Self::OnTheDay, Self::DayBefore, Self::TwoDaysBefore];

    /// The presets that make sense for a due date with a time, or for an all-day one.
    #[must_use]
    pub const fn for_kind(all_day: bool) -> &'static [Self] {
        if all_day {
            &Self::ALL_DAY
        } else {
            &Self::TIMED
        }
    }

    #[must_use]
    pub const fn applies(self, all_day: bool) -> bool {
        matches!(self, Self::OnTheDay | Self::DayBefore | Self::TwoDaysBefore) == all_day
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AtDue => "At due time",
            Self::TenMinutes => "10 minutes before",
            Self::OneHour => "1 hour before",
            Self::OneDay => "1 day before",
            Self::OnTheDay => "On the day, 09:00",
            Self::DayBefore => "The day before, 09:00",
            Self::TwoDaysBefore => "2 days before, 09:00",
        }
    }

    /// When the reminder fires for a due date. `None` if the preset is for the other kind of date.
    #[must_use]
    pub fn at(self, due: Time, all_day: bool, zone: &Zone) -> Option<Time> {
        if !self.applies(all_day) {
            return None;
        }
        let days_before = |n: i64| {
            let day = due_day(due, true, zone).add_days(-n)?;
            at_local(day, DAY_REMINDER_HOUR, 0, zone)
        };
        match self {
            Self::AtDue => Some(due),
            Self::TenMinutes => Some(due - Duration::minutes(10)),
            Self::OneHour => Some(due - Duration::hours(1)),
            Self::OneDay => Some(due - Duration::days(1)),
            Self::OnTheDay => days_before(0),
            Self::DayBefore => days_before(1),
            Self::TwoDaysBefore => days_before(2),
        }
    }

    /// Which preset gives exactly `remind` for this due date.
    #[must_use]
    pub fn detect(remind: Time, due: Time, all_day: bool, zone: &Zone) -> Option<Self> {
        Self::for_kind(all_day)
            .iter()
            .copied()
            .find(|p| p.at(due, all_day, zone) == Some(remind))
    }
}

/// A notification to schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    pub id: i32,
    pub task_id: String,
    pub at: Time,
    pub title: String,
    pub body: String,
}

/// A stable positive number for a task, as Android wants for a notification: the 32-bit FNV-1a of the id.
#[must_use]
pub fn notification_id(task_id: &str) -> i32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in task_id.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    i32::try_from(h & 0x7FFF_FFFF).unwrap_or(0)
}

/// "Due Wed 28 Oct 14:30" (no time for an all-day date), or an empty text with no due date.
#[must_use]
pub fn reminder_body(task: &Task, now: Time, zone: &Zone) -> String {
    let Some(due) = task.due_at else {
        return String::new();
    };
    let date: Date = due_day(due, task.due_all_day, zone);
    let day = format_date(date, today(now, zone));
    if task.due_all_day {
        format!("Due {day}")
    } else {
        let (h, m) = crate::calendar::clock_of(due, zone);
        format!("Due {day} {}", crate::calendar::format_clock(h, m))
    }
}

/// What to have scheduled: the open tasks whose reminder is still ahead, soonest first, at most [`MAX_SCHEDULED`].
#[must_use]
pub fn pending_reminders(tasks: &[Task], now: Time, zone: &Zone) -> Vec<Reminder> {
    let mut out: Vec<Reminder> = tasks
        .iter()
        .filter(|t| !t.is_completed)
        .filter_map(|t| {
            let at = t.remind_at.filter(|r| *r > now)?;
            Some(Reminder {
                id: notification_id(&t.id),
                task_id: t.id.clone(),
                at,
                title: t.content.clone(),
                body: reminder_body(t, now, zone),
            })
        })
        .collect();
    out.sort_by_key(|r| r.at);
    out.truncate(MAX_SCHEDULED);
    out
}
