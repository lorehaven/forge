//! The calls the controller makes to the services, and what they answer: plain data, so each host decides how to do them.

use daybook_core::{Data, Label, Project, Task};
use quench_serde::{Map, Value};

/// One call to the services.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Login {
        server: String,
        username: String,
        password: String,
    },
    /// The lists, the labels and the open tasks.
    Load,
    CreateTask(Value),
    UpdateTask(String, Value),
    DeleteTask(String),
    CreateProject {
        name: String,
        sort_order: i64,
    },
    RenameProject {
        id: String,
        name: String,
        sort_order: i64,
    },
    DeleteProject(String),
    CreateLabel {
        name: String,
        color: String,
    },
    UpdateLabel {
        id: String,
        name: String,
        color: String,
    },
    DeleteLabel(String),
    Logout,
}

/// What a call answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Done,
    Loaded(Box<Data>),
    Task(Box<Task>),
    Project(Project),
    Label(Label),
}

impl Op {
    /// Whether this is a change to the person's data (what is kept when the services cannot be reached).
    #[must_use]
    pub const fn is_change(&self) -> bool {
        !matches!(self, Self::Login { .. } | Self::Load | Self::Logout)
    }

    /// The change as one JSON value, for the outbox on the device. `None` for a call that is not a change.
    #[must_use]
    pub fn to_value(&self) -> Option<Value> {
        let mut m = Map::new();
        let text = |s: &str| Value::String(s.to_string());
        match self {
            Self::CreateTask(body) => {
                m.insert("op", text("create_task"));
                m.insert("body", body.clone());
            }
            Self::UpdateTask(id, body) => {
                m.insert("op", text("update_task"));
                m.insert("id", text(id));
                m.insert("body", body.clone());
            }
            Self::DeleteTask(id) => {
                m.insert("op", text("delete_task"));
                m.insert("id", text(id));
            }
            Self::CreateProject { name, sort_order } => {
                m.insert("op", text("create_project"));
                m.insert("name", text(name));
                m.insert("sort_order", Value::Int(*sort_order));
            }
            Self::RenameProject {
                id,
                name,
                sort_order,
            } => {
                m.insert("op", text("rename_project"));
                m.insert("id", text(id));
                m.insert("name", text(name));
                m.insert("sort_order", Value::Int(*sort_order));
            }
            Self::DeleteProject(id) => {
                m.insert("op", text("delete_project"));
                m.insert("id", text(id));
            }
            Self::CreateLabel { name, color } => {
                m.insert("op", text("create_label"));
                m.insert("name", text(name));
                m.insert("color", text(color));
            }
            Self::UpdateLabel { id, name, color } => {
                m.insert("op", text("update_label"));
                m.insert("id", text(id));
                m.insert("name", text(name));
                m.insert("color", text(color));
            }
            Self::DeleteLabel(id) => {
                m.insert("op", text("delete_label"));
                m.insert("id", text(id));
            }
            Self::Login { .. } | Self::Load | Self::Logout => return None,
        }
        Some(Value::Object(m))
    }

    /// What [`to_value`](Self::to_value) wrote.
    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
        let sort_order = || v.get("sort_order").and_then(Value::as_i64).unwrap_or(0);
        Some(match v.get("op")?.as_str()? {
            "create_task" => Self::CreateTask(v.get("body")?.clone()),
            "update_task" => Self::UpdateTask(text("id")?, v.get("body")?.clone()),
            "delete_task" => Self::DeleteTask(text("id")?),
            "create_project" => Self::CreateProject {
                name: text("name")?,
                sort_order: sort_order(),
            },
            "rename_project" => Self::RenameProject {
                id: text("id")?,
                name: text("name")?,
                sort_order: sort_order(),
            },
            "delete_project" => Self::DeleteProject(text("id")?),
            "create_label" => Self::CreateLabel {
                name: text("name")?,
                color: text("color")?,
            },
            "update_label" => Self::UpdateLabel {
                id: text("id")?,
                name: text("name")?,
                color: text("color")?,
            },
            "delete_label" => Self::DeleteLabel(text("id")?),
            _ => return None,
        })
    }

    /// Replaces the id `from` (something made offline, now known to the service as `to`) wherever this change names it.
    pub fn rename_id(&mut self, from: &str, to: &str) {
        let swap = |id: &mut String| {
            if id == from {
                *id = to.to_string();
            }
        };
        match self {
            Self::CreateTask(body) => rename_in_body(body, from, to),
            Self::UpdateTask(id, body) => {
                swap(id);
                rename_in_body(body, from, to);
            }
            Self::DeleteTask(id)
            | Self::RenameProject { id, .. }
            | Self::DeleteProject(id)
            | Self::UpdateLabel { id, .. }
            | Self::DeleteLabel(id) => swap(id),
            Self::CreateProject { .. }
            | Self::CreateLabel { .. }
            | Self::Login { .. }
            | Self::Load
            | Self::Logout => {}
        }
    }
}

/// A task body names a list (`project_id`) and labels (`label_ids`).
fn rename_in_body(body: &mut Value, from: &str, to: &str) {
    let Value::Object(m) = body else { return };
    let same = |v: &Value| v.as_str() == Some(from);
    if m.get("project_id").is_some_and(same) {
        m.insert("project_id", Value::String(to.to_string()));
    }
    if let Some(Value::Array(ids)) = m.get("label_ids") {
        let ids = ids
            .iter()
            .map(|i| {
                if same(i) {
                    Value::String(to.to_string())
                } else {
                    i.clone()
                }
            })
            .collect();
        m.insert("label_ids", Value::Array(ids));
    }
}
