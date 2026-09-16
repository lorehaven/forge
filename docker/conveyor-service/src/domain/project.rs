//! A node in conveyor's organisational tree - no separate "group" type; container-vs-leaf is
//! just what's attached, and nesting is unbounded.

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
