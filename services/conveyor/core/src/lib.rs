//! What Conveyor's API says and how a form is read: the records, the wire models, the drafts, the project tree, the job
//! graph and the formatting. No I/O: the service and the browser client share it.

pub mod domain;
pub mod draft;
pub mod format;
pub mod graph;
pub mod model;
pub mod scan;
pub mod tree;

pub use domain::{Artifact, Job, Project, Provider, Repo, Run, Status, Trigger};
pub use draft::{DraftError, LaunchDraft, RepoDraft};
pub use model::{
    Access, ArtifactView, CheckChip, CheckDetail, Credential, Finding, KnownStage, RegisterRepo,
    RepoChecks, RunDetail, RunView, RunsPage, ScanFinding, ScanFindings, ScanReport, Subscription,
    TriggerRun, UpdateRepo,
};
