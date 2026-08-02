//! A repository conveyor is willing to build.
//!
//! Registration is explicit. Conveyor runs code that a repository supplies, so
//! "any webhook that arrives" is not an acceptable trigger - a repo has to be
//! added here before a delivery for it is worth verifying.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    GitHub,
    /// No provider-specific behaviour: a shared-secret webhook and a plain
    /// clone, with nothing reported back.
    Generic,
}

impl Provider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::Generic => "generic",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "github" => Some(Self::GitHub),
            "generic" => Some(Self::Generic),
            _ => None,
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Repo {
    pub id: String,
    pub provider: Provider,
    /// Organisation or user, as the provider names it. Independent of
    /// `project_id`: this is the provider's identity for the repo (what a
    /// webhook slug names), not its place in conveyor's own tree.
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    pub default_branch: String,
    /// The realm account that registered it, and who its secrets belong to.
    pub registered_by: String,
    /// Where this repo sits in conveyor's organisational tree.
    pub project_id: String,
    /// A disabled repo keeps its history and stops accepting triggers.
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Repo {
    /// `owner/name`, the form used in logs, the UI and provider API paths.
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}
