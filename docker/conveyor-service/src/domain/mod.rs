//! The records conveyor keeps, and the vocabulary the rest of the service
//! speaks in.
//!
//! These types mirror the `conveyor` schema installed by foundry
//! (`docker/foundry-service/migrations/conveyor/`). They are defined alongside
//! that migration deliberately: a status string that exists in one and not the
//! other is the kind of drift nobody notices until a run is stuck.

pub mod artifact;
pub mod job;
pub mod project;
pub mod repo;
pub mod run;
pub mod status;
pub mod step;

pub use artifact::Artifact;
pub use job::Job;
pub use project::Project;
pub use repo::{Provider, Repo};
pub use run::{Run, Trigger};
pub use status::Status;
pub use step::StepRecord;
