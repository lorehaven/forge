//! What the home-screen widget draws, as one JSON text the widget only filters: the lists it can show and every open
//! task once, in display order. A due time travels as milliseconds plus the all-day flag, and the widget works out "Today
//! 14:30" and its colour when it draws, so a snapshot that is hours old still says "Tomorrow" correctly after midnight.

use quench_core::time::Time;
use quench_serde::{Map, Value, json};
use quench_tz::Zone;

use crate::data::Data;
use crate::order::order_tasks;

/// The most tasks a snapshot carries: a widget shows a screenful, this bounds what is handed to the launcher.
pub const MAX_TASKS: usize = 300;

/// The id the widget uses for a list: `inbox`, `p:<project id>` or `l:<label id>`.
#[must_use]
pub fn list_id(list: &crate::model::ListRef) -> String {
    match list {
        crate::model::ListRef::Inbox => "inbox".into(),
        crate::model::ListRef::Project(id) => format!("p:{id}"),
        crate::model::ListRef::Label(id) => format!("l:{id}"),
    }
}

/// The inverse of [`list_id`]; `None` for something that is not one.
#[must_use]
pub fn parse_list_id(id: &str) -> Option<crate::model::ListRef> {
    match id {
        "inbox" => Some(crate::model::ListRef::Inbox),
        _ => match id.split_once(':') {
            Some(("p", rest)) if !rest.is_empty() => {
                Some(crate::model::ListRef::Project(rest.into()))
            }
            Some(("l", rest)) if !rest.is_empty() => {
                Some(crate::model::ListRef::Label(rest.into()))
            }
            _ => None,
        },
    }
}

fn entry(pairs: &[(&str, Value)]) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        m.insert(*k, v.clone());
    }
    Value::Object(m)
}

fn text(s: &str) -> Value {
    Value::String(s.to_string())
}

/// The snapshot as JSON text.
#[must_use]
pub fn snapshot(data: &Data, now: Time, zone: &Zone) -> String {
    let mut lists = vec![entry(&[
        ("id", text("inbox")),
        ("name", text("Inbox")),
        ("kind", text("inbox")),
    ])];
    lists.extend(data.projects.iter().map(|p| {
        entry(&[
            ("id", text(&format!("p:{}", p.id))),
            ("name", text(&p.name)),
            ("kind", text("project")),
        ])
    }));
    lists.extend(data.labels.iter().map(|l| {
        entry(&[
            ("id", text(&format!("l:{}", l.id))),
            ("name", text(&l.name)),
            ("kind", text("label")),
            ("color", text(&l.color)),
        ])
    }));
    let open: Vec<_> = data
        .tasks
        .iter()
        .filter(|t| !t.is_completed)
        .cloned()
        .collect();
    let tasks: Vec<Value> = order_tasks(&open, zone)
        .iter()
        .take(MAX_TASKS)
        .map(|t| {
            entry(&[
                ("id", text(&t.id)),
                ("content", text(&t.content)),
                ("project", t.project_id.as_deref().map_or(Value::Null, text)),
                (
                    "labels",
                    Value::Array(t.label_ids.iter().map(|l| text(l)).collect()),
                ),
                (
                    "due",
                    t.due_at
                        .map_or(Value::Null, |d| Value::Int(d.timestamp_millis())),
                ),
                ("allDay", Value::Bool(t.due_all_day)),
                ("repeats", Value::Bool(t.recurrence.is_some())),
            ])
        })
        .collect();
    json::to_string(&entry(&[
        ("version", Value::Int(1)),
        ("generated", Value::Int(now.timestamp_millis())),
        ("lists", Value::Array(lists)),
        ("tasks", Value::Array(tasks)),
    ]))
}
