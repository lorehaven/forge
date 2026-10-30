//! A node in conveyor's organisational tree - no separate "group" type; container-vs-leaf is
//! just what's attached, and nesting is unbounded.

use quench_core::time::Time;
use quench_serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// `None` for a root node.
    pub parent_id: Option<String>,
    pub created_at: Time,
    pub updated_at: Time,
}
