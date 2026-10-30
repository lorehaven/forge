//! The records conveyor keeps.
//!
//! They live in `conveyor-core` (the browser client reads the same ones); this re-exports them. Mirrors the `conveyor` schema foundry installs (`services/foundry/service/migrations/conveyor/`) - keep both in sync.

pub use conveyor_core::domain::{
    Artifact, Job, Project, Provider, Repo, Run, Status, Trigger, artifact, job, project, repo,
    run, status,
};
pub mod step;
pub use step::StepRecord;
