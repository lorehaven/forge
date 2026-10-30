//! The JSON the API speaks, beyond the records themselves.

use crate::domain::{Artifact, Job, Repo, Run};
use quench_core::time::Time;
use quench_serde::{Deserialize, Serialize};

/// What the caller may do, per project (a grant covers the project and everything nested beneath it).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Access {
    pub username: String,
    /// Ids of the projects the caller may read.
    pub read: Vec<String>,
    /// Ids of the projects the caller may write to.
    pub write: Vec<String>,
}

impl Access {
    #[must_use]
    pub fn can_read(&self, project_id: &str) -> bool {
        self.read.iter().any(|id| id == project_id)
    }

    #[must_use]
    pub fn can_write(&self, project_id: &str) -> bool {
        self.write.iter().any(|id| id == project_id)
    }
}

/// One page of the full run history.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunsPage {
    pub runs: Vec<Run>,
    pub page: u32,
    /// At least 1, even for an empty history.
    pub pages: u32,
    pub total: i64,
}

/// An artifact with the address a browser can follow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactView {
    #[serde(flatten)]
    pub artifact: Artifact,
    pub href: String,
}

/// `GET /api/v1/runs/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunDetail {
    #[serde(flatten)]
    pub run: Run,
    pub jobs: Vec<Job>,
    pub artifacts: Vec<ArtifactView>,
}

/// A run's page: the run, its jobs and artifacts, and its repository when it is still there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunView {
    pub run: Run,
    pub jobs: Vec<Job>,
    pub artifacts: Vec<ArtifactView>,
    pub repo: Option<Repo>,
}

/// One check of a repository as a chip shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckChip {
    /// `lint`, `machete`, `audit`, `coverage`, `tests` or `security`.
    pub kind: String,
    pub passed: bool,
    pub findings: usize,
    pub metric: Option<String>,
}

/// The chips of one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoChecks {
    pub repo_id: String,
    pub checks: Vec<CheckChip>,
}

/// One thing a check found.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub title: String,
    pub id: Option<String>,
    pub severity: Option<String>,
    pub date: Option<String>,
    pub location: Option<String>,
    pub extra: Option<String>,
}

/// One check of the latest scan, in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckDetail {
    pub kind: String,
    pub job_name: String,
    pub passed: bool,
    pub headline: String,
    pub findings: Vec<Finding>,
    pub metric: Option<String>,
    /// Whether the check keeps a stored report page (tests, security).
    pub has_report: bool,
    /// The run this result came from.
    pub origin: Option<Run>,
}

/// A repository's code-quality summary.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReport {
    /// The newest run, whatever it did.
    pub run: Option<Run>,
    pub checks: Vec<CheckDetail>,
}

impl ScanReport {
    #[must_use]
    pub fn check(&self, kind: &str) -> Option<&CheckDetail> {
        self.checks.iter().find(|c| c.kind == kind)
    }
}

/// A finding of the security scanner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanFinding {
    pub severity: String,
    pub check: String,
    pub service: String,
    pub method: String,
    pub path: String,
    pub detail: String,
}

/// The scanner's findings of the newest scan, and when it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanFindings {
    pub findings: Vec<ScanFinding>,
    pub run_id: String,
    pub created_at: Time,
}

/// A credential, without its token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    pub id: String,
    pub project_id: Option<String>,
    pub repo_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub username: String,
    pub preview: String,
    pub created_by: String,
    pub created_at: Time,
    pub updated_at: Time,
}

/// A follow of a repository or a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub repo_id: Option<String>,
    pub project_id: Option<String>,
    pub created_at: Time,
}

/// A stage of a repository's pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownStage {
    pub name: String,
    /// Whether an ordinary run includes it.
    pub on_by_default: bool,
}

/// `POST /api/v1/repos`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterRepo {
    pub provider: Option<String>,
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    pub default_branch: Option<String>,
    pub project_id: String,
}

/// `PATCH /api/v1/repos/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRepo {
    pub owner: Option<String>,
    pub name: Option<String>,
    pub clone_url: Option<String>,
    pub default_branch: Option<String>,
    pub project_id: Option<String>,
    pub enabled: Option<bool>,
}

/// `POST /api/v1/repos/{id}/runs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerRun {
    pub git_ref: Option<String>,
    pub sha: Option<String>,
    pub stages: Option<Vec<String>>,
}
