//! The records conveyor keeps and its API speaks. Mirrors the `conveyor` schema foundry installs
//! (`services/foundry/service/migrations/conveyor/`) - keep both in sync.

pub mod artifact;
pub mod job;
pub mod project;
pub mod repo;
pub mod run;
pub mod status;

pub use artifact::Artifact;
pub use job::Job;
pub use project::Project;
pub use repo::{Provider, Repo};
pub use run::{Run, Trigger};
pub use status::Status;
