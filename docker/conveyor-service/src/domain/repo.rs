//! A repository conveyor is willing to build. Registration is explicit - a repo must be added
//! here before a webhook delivery for it is worth verifying.

use quench_core::time::Time;
use quench_serde::{Deserialize, Serialize};
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
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::Generic => "generic",
        }
    }

    #[must_use]
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
    /// The provider's identity for the repo (what a webhook slug names) - independent of `project_id`.
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
    pub created_at: Time,
    pub updated_at: Time,
}

impl Repo {
    /// `owner/name`, the form used in logs, the UI and provider API paths.
    #[must_use]
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}
