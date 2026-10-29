//! The calls the controller makes to the services, and what they answer: plain data, so each host decides how to do them.

use daybook_core::{Data, Label, Project, Task};
use quench_serde::Value;

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
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Done,
    Loaded(Box<Data>),
    Task(Box<Task>),
    Project(Project),
    Label(Label),
}
