//! The rules of the task form, with no screen: what editing the due date does to the reminder and the repeat.

use quench_core::date::Date;
use quench_core::time::Time;
use quench_tz::Zone;

use crate::calendar::{all_day_time, at_local, clock_of, due_day};
use crate::model::{Fields, ListRef, Recurrence, Task, task_body};
use crate::reminders::Preset;
use quench_serde::Value;

/// A due date: a day, with a time of day or without (all day).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Due {
    pub date: Date,
    /// Hour and minute; `None` is all day.
    pub clock: Option<(u32, u32)>,
}

impl Due {
    #[must_use]
    pub const fn all_day(&self) -> bool {
        self.clock.is_none()
    }

    /// The instant stored for it.
    #[must_use]
    pub fn instant(&self, zone: &Zone) -> Option<Time> {
        match self.clock {
            None => Some(all_day_time(self.date)),
            Some((h, m)) => at_local(self.date, h, m, zone),
        }
    }
}

/// When to be reminded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remind {
    None,
    /// Follows the due date.
    Preset(Preset),
    /// A date and time of its own.
    At(Time),
}

/// A task being made or edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub content: String,
    pub description: String,
    pub project_id: Option<String>,
    pub label_ids: Vec<String>,
    due: Option<Due>,
    remind: Remind,
    recurrence: Option<Recurrence>,
}

impl Draft {
    /// A new task in the view being looked at (a list's own, a label's tag, or the inbox).
    #[must_use]
    pub fn new(list: &ListRef) -> Self {
        Self {
            content: String::new(),
            description: String::new(),
            project_id: list.project_id().map(str::to_string),
            label_ids: list
                .label_id()
                .map(|l| vec![l.to_string()])
                .unwrap_or_default(),
            due: None,
            remind: Remind::None,
            recurrence: None,
        }
    }

    #[must_use]
    pub fn from_task(task: &Task, zone: &Zone) -> Self {
        let due = task.due_at.map(|d| Due {
            date: due_day(d, task.due_all_day, zone),
            clock: (!task.due_all_day).then(|| clock_of(d, zone)),
        });
        let remind = match (task.remind_at, task.due_at) {
            (Some(r), Some(d)) => {
                Preset::detect(r, d, task.due_all_day, zone).map_or(Remind::At(r), Remind::Preset)
            }
            (Some(r), None) => Remind::At(r),
            (None, _) => Remind::None,
        };
        Self {
            content: task.content.clone(),
            description: task.description.clone().unwrap_or_default(),
            project_id: task.project_id.clone(),
            label_ids: task.label_ids.clone(),
            due,
            remind,
            recurrence: task.recurrence.clone(),
        }
    }

    #[must_use]
    pub const fn due(&self) -> Option<Due> {
        self.due
    }

    #[must_use]
    pub const fn remind(&self) -> Remind {
        self.remind
    }

    #[must_use]
    pub const fn recurrence(&self) -> Option<&Recurrence> {
        self.recurrence.as_ref()
    }

    /// Sets the due date. A reminder that follows it moves with it; one that is for the other kind of date (a time against
    /// all day) is dropped.
    pub fn set_due(&mut self, due: Due) {
        if let Remind::Preset(p) = self.remind
            && !p.applies(due.all_day())
        {
            self.remind = Remind::None;
        }
        self.due = Some(due);
    }

    /// No due date: nothing repeats without one, and a reminder that followed it has nothing to follow.
    pub fn clear_due(&mut self) {
        self.due = None;
        self.recurrence = None;
        if matches!(self.remind, Remind::Preset(_)) {
            self.remind = Remind::None;
        }
    }

    /// Chooses a preset; needs a due date of the right kind.
    pub fn set_preset(&mut self, preset: Preset) -> bool {
        match self.due {
            Some(due) if preset.applies(due.all_day()) => {
                self.remind = Remind::Preset(preset);
                true
            }
            _ => false,
        }
    }

    pub const fn set_reminder_at(&mut self, at: Time) {
        self.remind = Remind::At(at);
    }

    pub const fn clear_reminder(&mut self) {
        self.remind = Remind::None;
    }

    /// Sets how the task repeats; needs a due date (the service refuses one without). Returns whether it was taken.
    pub fn set_recurrence(&mut self, recurrence: Option<Recurrence>) -> bool {
        if recurrence.is_some() && self.due.is_none() {
            return false;
        }
        self.recurrence = recurrence;
        true
    }

    pub fn toggle_label(&mut self, id: &str) {
        if let Some(i) = self.label_ids.iter().position(|l| l == id) {
            self.label_ids.remove(i);
        } else {
            self.label_ids.push(id.to_string());
        }
    }

    /// Whether the form may be saved: it needs a title.
    #[must_use]
    pub fn can_save(&self) -> bool {
        !self.content.trim().is_empty()
    }

    /// When the reminder fires.
    #[must_use]
    pub fn remind_at(&self, zone: &Zone) -> Option<Time> {
        match self.remind {
            Remind::None => None,
            Remind::At(t) => Some(t),
            Remind::Preset(p) => {
                let due = self.due?;
                p.at(due.instant(zone)?, due.all_day(), zone)
            }
        }
    }

    /// The JSON to send. `is_completed` and `sort_order` are those of the task being replaced (a new one: open, 0).
    #[must_use]
    pub fn body(&self, zone: &Zone, is_completed: Option<bool>, sort_order: i64) -> Value {
        let description = self.description.trim();
        let due_at = self.due.and_then(|d| d.instant(zone));
        task_body(&Fields {
            content: self.content.trim(),
            description: (!description.is_empty()).then_some(description),
            project_id: self.project_id.as_deref(),
            is_completed,
            sort_order,
            due_at,
            due_all_day: self.due.is_some_and(|d| d.all_day()),
            remind_at: self.remind_at(zone),
            label_ids: &self.label_ids,
            recurrence: self.recurrence.as_ref(),
        })
    }
}
