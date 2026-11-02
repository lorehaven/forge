//! Everything the app shows, and the local changes that are made before the service has answered.

use quench_core::time::Time;
use quench_serde::{Map, Value};

use crate::model::{Label, Project, Task};

/// The person's lists, labels and tasks as the app last knew them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Data {
    pub projects: Vec<Project>,
    pub labels: Vec<Label>,
    /// Open tasks always; completed ones only when they were asked for.
    pub tasks: Vec<Task>,
}

impl Data {
    #[must_use]
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// Puts a task in, in the place of the one with its id.
    pub fn put_task(&mut self, task: Task) {
        match self.tasks.iter_mut().find(|t| t.id == task.id) {
            Some(slot) => *slot = task,
            None => self.tasks.push(task),
        }
    }

    /// Takes a task out and returns it.
    pub fn take_task(&mut self, id: &str) -> Option<Task> {
        let at = self.tasks.iter().position(|t| t.id == id)?;
        Some(self.tasks.remove(at))
    }

    /// Marks a task done or open at `now`, as the person sees it before the service answers.
    pub fn mark(&mut self, id: &str, done: bool, now: Time) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.is_completed = done;
            t.updated_at = now;
        }
    }

    /// Takes a list out with its tasks, as the service does.
    pub fn remove_project(&mut self, id: &str) {
        self.projects.retain(|p| p.id != id);
        self.tasks.retain(|t| t.project_id.as_deref() != Some(id));
    }

    /// Takes a label out of the labels and off every task.
    pub fn remove_label(&mut self, id: &str) {
        self.labels.retain(|l| l.id != id);
        for t in &mut self.tasks {
            t.label_ids.retain(|l| l != id);
        }
    }

    pub fn put_label(&mut self, label: Label) {
        match self.labels.iter_mut().find(|l| l.id == label.id) {
            Some(slot) => *slot = label,
            None => self.labels.push(label),
        }
        self.labels.sort_by_key(|l| l.name.to_lowercase());
    }

    pub fn put_project(&mut self, project: Project) {
        match self.projects.iter_mut().find(|p| p.id == project.id) {
            Some(slot) => *slot = project,
            None => self.projects.push(project),
        }
        self.projects.sort_by(|a, b| {
            a.sort_order
                .cmp(&b.sort_order)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }

    /// Everything as one JSON value, for the copy kept on the device.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert(
            "projects",
            Value::Array(self.projects.iter().map(Project::to_value).collect()),
        );
        m.insert(
            "labels",
            Value::Array(self.labels.iter().map(Label::to_value).collect()),
        );
        m.insert(
            "tasks",
            Value::Array(self.tasks.iter().map(Task::to_value).collect()),
        );
        Value::Object(m)
    }

    /// What [`to_value`](Self::to_value) wrote.
    #[must_use]
    pub fn from_value(v: &Value) -> Self {
        fn rows<T>(v: &Value, key: &str, parse: fn(&Value) -> Option<T>) -> Vec<T> {
            v.get(key)
                .and_then(Value::as_array)
                .map(|rows| rows.iter().filter_map(parse).collect())
                .unwrap_or_default()
        }
        Self {
            projects: rows(v, "projects", Project::from_value),
            labels: rows(v, "labels", Label::from_value),
            tasks: rows(v, "tasks", Task::from_value),
        }
    }

    /// Replaces the id `from` (a row made while offline) by `to`, in the rows and in what refers to them.
    pub fn rename_id(&mut self, from: &str, to: &str) {
        for p in &mut self.projects {
            if p.id == from {
                p.id = to.to_string();
            }
        }
        for l in &mut self.labels {
            if l.id == from {
                l.id = to.to_string();
            }
        }
        for t in &mut self.tasks {
            if t.id == from {
                t.id = to.to_string();
            }
            if t.project_id.as_deref() == Some(from) {
                t.project_id = Some(to.to_string());
            }
            for l in &mut t.label_ids {
                if l == from {
                    *l = to.to_string();
                }
            }
        }
    }

    /// The tasks that are not completed.
    pub fn open(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().filter(|t| !t.is_completed)
    }
}
