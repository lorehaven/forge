//! The JSON the API speaks. Timestamps stay RFC 3339 text: the browser only shows them.

use quench_serde::{Deserialize, Serialize};

/// The workflow, in the order of the board's columns.
pub const STATUSES: [&str; 5] = ["blocked", "todo", "in-progress", "done", "rejected"];
pub const KINDS: [&str; 3] = ["task", "bug", "story"];
pub const PRIORITIES: [&str; 3] = ["low", "medium", "high"];
/// What the link form offers; the API also reads a link's other side as "blocked by".
pub const LINK_KINDS: [&str; 2] = ["blocks", "relates_to"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Project {
    /// `WB — Workbench`, as the pages title a project.
    #[must_use]
    pub fn title(&self) -> String {
        format!("{} — {}", self.key, self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub id: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub seq: i32,
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: String,
    pub assignee: Option<String>,
    pub reporter: String,
    pub estimate: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
}

impl Issue {
    /// `WB-4`.
    #[must_use]
    pub fn key(&self, project_key: &str) -> String {
        format!("{project_key}-{}", self.seq)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub issue_id: String,
    pub author: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkedIssue {
    pub link_id: String,
    pub issue_id: String,
    pub project_key: String,
    pub seq: i32,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedIssues {
    pub blocks: Vec<LinkedIssue>,
    pub blocked_by: Vec<LinkedIssue>,
    pub relates_to: Vec<LinkedIssue>,
}

/// Someone an issue can be assigned to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub display_name: Option<String>,
}

impl User {
    /// The display name if there is one, else the username.
    #[must_use]
    pub fn label(&self) -> &str {
        self.display_name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.username)
    }
}

/// Who is asking (`GET /api/v1/me`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Me {
    pub username: String,
}

/// A project with its issues: what the board shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub project: Project,
    pub issues: Vec<Issue>,
}

/// An issue with everything its page shows (the client puts it together from several calls).
#[derive(Debug, Clone, PartialEq)]
pub struct IssueView {
    pub issue: Issue,
    pub project: Project,
    pub comments: Vec<Comment>,
    pub links: RelatedIssues,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateProject {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateIssue {
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: String,
    pub assignee: Option<String>,
    pub estimate: Option<i32>,
}

/// `PUT /issues/{id}` replaces every field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateIssue {
    pub title: String,
    pub description: Option<String>,
    pub kind: String,
    pub priority: String,
    pub assignee: Option<String>,
    pub estimate: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewComment {
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewLink {
    pub linked_issue_id: String,
    pub kind: String,
}
