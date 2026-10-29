//! What the service sends, and what is sent back.

use quench_core::time::Time;
use quench_serde::{Map, Value};

/// A user's own list. Tasks outside any project are in the inbox, which is not a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
}

impl Project {
    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            id: text(v, "id")?,
            name: text(v, "name")?,
            sort_order: v.get("sort_order").and_then(Value::as_i64).unwrap_or(0),
        })
    }
}

/// A user's own label: it can tag tasks of the inbox and of every list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub id: String,
    pub name: String,
    /// `#rrggbb`.
    pub color: String,
}

/// What the service uses when a label is made with no colour.
pub const DEFAULT_LABEL_COLOR: &str = "#888888";

/// The colours the label dialog offers (the service takes any `#rrggbb`).
pub const LABEL_COLORS: [&str; 9] = [
    "#ef4444",
    "#f97316",
    "#eab308",
    "#22c55e",
    "#14b8a6",
    "#3b82f6",
    "#8b5cf6",
    "#ec4899",
    DEFAULT_LABEL_COLOR,
];

impl Label {
    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            id: text(v, "id")?,
            name: text(v, "name")?,
            color: text(v, "color").unwrap_or_else(|| DEFAULT_LABEL_COLOR.to_string()),
        })
    }

    /// The colour as red, green and blue; anything that is not `#rrggbb` is grey.
    #[must_use]
    pub fn rgb(&self) -> (u8, u8, u8) {
        parse_hex(&self.color)
    }
}

/// `#rrggbb` as red, green and blue; grey when it is anything else.
#[must_use]
pub fn parse_hex(hex: &str) -> (u8, u8, u8) {
    let digits = hex.strip_prefix('#').unwrap_or(hex);
    match (digits.len() == 6)
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
    {
        Some(v) => (
            ((v >> 16) & 255) as u8,
            ((v >> 8) & 255) as u8,
            (v & 255) as u8,
        ),
        None => (0x88, 0x88, 0x88),
    }
}

/// What a repeat counts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Day,
    Week,
    Month,
    Year,
}

impl Unit {
    pub const ALL: [Self; 4] = [Self::Day, Self::Week, Self::Month, Self::Year];

    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    #[must_use]
    pub const fn plural(self) -> &'static str {
        match self {
            Self::Day => "days",
            Self::Week => "weeks",
            Self::Month => "months",
            Self::Year => "years",
        }
    }

    #[must_use]
    pub fn parse(wire: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|u| u.wire() == wire)
            .unwrap_or(Self::Day)
    }
}

/// What the next date is counted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The schedule: a Monday task stays on Mondays however late it is done.
    Fixed,
    /// The day it is done: "3 days after the last time".
    AfterCompletion,
}

impl Mode {
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::AfterCompletion => "after_completion",
        }
    }
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// How a repeating task comes back. The service applies it: completing the task returns it open at its next date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recurrence {
    pub unit: Unit,
    /// Every `interval` units, 1 to 999.
    pub interval: u32,
    /// ISO weekdays, 1 = Monday to 7 = Sunday. Only for a fixed weekly rule; empty means the weekday the task is due on.
    pub weekdays: Vec<u8>,
    pub mode: Mode,
    /// The IANA zone the rule is evaluated in.
    pub timezone: String,
}

impl Recurrence {
    /// Every day, week, month or year, in `timezone`.
    #[must_use]
    pub fn every(unit: Unit, timezone: &str) -> Self {
        Self {
            unit,
            interval: 1,
            weekdays: Vec::new(),
            mode: Mode::Fixed,
            timezone: timezone.to_string(),
        }
    }

    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        v.as_object()?;
        Some(Self {
            unit: Unit::parse(v.get("unit").and_then(Value::as_str).unwrap_or("day")),
            interval: v
                .get("interval")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(1),
            weekdays: v
                .get("weekdays")
                .and_then(Value::as_array)
                .map(|days| {
                    days.iter()
                        .filter_map(|d| d.as_u64().and_then(|n| u8::try_from(n).ok()))
                        .collect()
                })
                .unwrap_or_default(),
            mode: if v.get("mode").and_then(Value::as_str) == Some("after_completion") {
                Mode::AfterCompletion
            } else {
                Mode::Fixed
            },
            timezone: text(v, "timezone").unwrap_or_else(|| "UTC".to_string()),
        })
    }

    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("unit", Value::String(self.unit.wire().into()));
        m.insert("interval", Value::UInt(u64::from(self.interval)));
        m.insert(
            "weekdays",
            Value::Array(
                self.weekdays
                    .iter()
                    .map(|d| Value::UInt(u64::from(*d)))
                    .collect(),
            ),
        );
        m.insert("mode", Value::String(self.mode.wire().into()));
        m.insert("timezone", Value::String(self.timezone.clone()));
        Value::Object(m)
    }

    /// "Every day", "Every 2 weeks on Mon, Thu", "3 days after completion".
    #[must_use]
    pub fn describe(&self) -> String {
        let unit = if self.interval == 1 {
            self.unit.wire()
        } else {
            self.unit.plural()
        };
        if self.mode == Mode::AfterCompletion {
            return format!("{} {unit} after completion", self.interval);
        }
        let every = if self.interval == 1 {
            format!("Every {unit}")
        } else {
            format!("Every {} {unit}", self.interval)
        };
        if self.weekdays.is_empty() {
            return every;
        }
        let days: Vec<&str> = self
            .weekdays
            .iter()
            .map(|d| WEEKDAYS[usize::from(d.saturating_sub(1)) % 7])
            .collect();
        format!("{every} on {}", days.join(", "))
    }
}

/// One task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    /// `None` is the inbox.
    pub project_id: Option<String>,
    pub content: String,
    pub description: Option<String>,
    pub is_completed: bool,
    pub sort_order: i64,
    pub due_at: Option<Time>,
    /// The due date has no time of day: `due_at` is midnight UTC of the date, and the date is that UTC date in any zone.
    pub due_all_day: bool,
    /// When to notify. The service only stores it; the app delivers it.
    pub remind_at: Option<Time>,
    /// In the order of the labels' names, as the service returns them.
    pub label_ids: Vec<String>,
    /// Set for a repeating task, which completing does not close: it comes back open, due at its next date.
    pub recurrence: Option<Recurrence>,
    pub created_at: Time,
    pub updated_at: Time,
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn time(v: &Value, key: &str) -> Option<Time> {
    v.get(key)
        .and_then(Value::as_str)
        .and_then(|t| Time::parse_rfc3339(t).ok())
}

fn time_value(t: Option<Time>) -> Value {
    t.map_or(Value::Null, |t| Value::String(t.to_rfc3339()))
}

impl Task {
    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        let now = Time::from_unix(0);
        Some(Self {
            id: text(v, "id")?,
            project_id: text(v, "project_id"),
            content: text(v, "content")?,
            description: text(v, "description"),
            is_completed: matches!(v.get("is_completed"), Some(Value::Bool(true))),
            sort_order: v.get("sort_order").and_then(Value::as_i64).unwrap_or(0),
            due_at: time(v, "due_at"),
            due_all_day: matches!(v.get("due_all_day"), Some(Value::Bool(true))),
            remind_at: time(v, "remind_at"),
            label_ids: v
                .get("label_ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|i| i.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            recurrence: v.get("recurrence").and_then(Recurrence::from_value),
            created_at: time(v, "created_at").unwrap_or(now),
            updated_at: time(v, "updated_at").unwrap_or(now),
        })
    }

    /// The body of `PUT /tasks/{id}`: the endpoint replaces the task, so every field is sent (leaving one out resets it).
    #[must_use]
    pub fn to_update(&self) -> Value {
        task_body(&Fields {
            content: &self.content,
            description: self.description.as_deref(),
            project_id: self.project_id.as_deref(),
            is_completed: Some(self.is_completed),
            sort_order: self.sort_order,
            due_at: self.due_at,
            due_all_day: self.due_all_day,
            remind_at: self.remind_at,
            label_ids: &self.label_ids,
            recurrence: self.recurrence.as_ref(),
        })
    }
}

/// The fields of a task as they are sent.
pub struct Fields<'a> {
    pub content: &'a str,
    pub description: Option<&'a str>,
    pub project_id: Option<&'a str>,
    /// Sent on an update only.
    pub is_completed: Option<bool>,
    pub sort_order: i64,
    pub due_at: Option<Time>,
    pub due_all_day: bool,
    pub remind_at: Option<Time>,
    pub label_ids: &'a [String],
    pub recurrence: Option<&'a Recurrence>,
}

/// The JSON for a task, to create (`is_completed` left out) or to replace.
#[must_use]
pub fn task_body(f: &Fields<'_>) -> Value {
    let mut m = Map::new();
    m.insert("content", Value::String(f.content.to_string()));
    m.insert(
        "description",
        f.description
            .map_or(Value::Null, |d| Value::String(d.to_string())),
    );
    m.insert(
        "project_id",
        f.project_id
            .map_or(Value::Null, |p| Value::String(p.to_string())),
    );
    if let Some(done) = f.is_completed {
        m.insert("is_completed", Value::Bool(done));
    }
    m.insert("sort_order", Value::Int(f.sort_order));
    m.insert("due_at", time_value(f.due_at));
    m.insert("due_all_day", Value::Bool(f.due_all_day));
    m.insert("remind_at", time_value(f.remind_at));
    m.insert(
        "label_ids",
        Value::Array(
            f.label_ids
                .iter()
                .map(|i| Value::String(i.clone()))
                .collect(),
        ),
    );
    m.insert(
        "recurrence",
        f.recurrence.map_or(Value::Null, Recurrence::to_value),
    );
    Value::Object(m)
}

/// Which tasks are being looked at: the inbox, one list, or every task with one label (across all lists).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ListRef {
    #[default]
    Inbox,
    Project(String),
    Label(String),
}

impl ListRef {
    /// Whether `task` belongs in this view.
    #[must_use]
    pub fn contains(&self, task: &Task) -> bool {
        match self {
            Self::Inbox => task.project_id.is_none(),
            Self::Project(id) => task.project_id.as_deref() == Some(id),
            Self::Label(id) => task.label_ids.iter().any(|l| l == id),
        }
    }

    #[must_use]
    pub fn project_id(&self) -> Option<&str> {
        match self {
            Self::Project(id) => Some(id),
            _ => None,
        }
    }

    #[must_use]
    pub fn label_id(&self) -> Option<&str> {
        match self {
            Self::Label(id) => Some(id),
            _ => None,
        }
    }

    /// A short text for a file (`inbox`, `project:<id>`, `label:<id>`) and `parse` back.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Inbox => "inbox".into(),
            Self::Project(id) => format!("project:{id}"),
            Self::Label(id) => format!("label:{id}"),
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Self {
        match key.split_once(':') {
            Some(("project", id)) if !id.is_empty() => Self::Project(id.into()),
            Some(("label", id)) if !id.is_empty() => Self::Label(id.into()),
            _ => Self::Inbox,
        }
    }
}
