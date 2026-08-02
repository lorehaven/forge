//! A node in conveyor's organisational tree.
//!
//! There is deliberately no separate "group" type. A node may have children,
//! may have a repository attached (`Repo::project_id`), both, or neither -
//! whether it reads as a container or a leaf falls out of what is attached to
//! it, not out of a column here. Nesting is unbounded: a node's parent is
//! itself just a node.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// `None` for a root node.
    pub parent_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
