//! Getting from a queued run to a finished one.
//!
//! The plan called for a `concurrency` module holding the per-repository rule.
//! It is not here: the rule turned out to belong in the claim query and in a
//! partial unique index beside it, where it holds across replicas. A module
//! enforcing it in one process would have been true only of that process.

pub mod projects;
pub mod queue;
pub mod repos;
pub mod worker;

pub use projects::NewProject;
pub use queue::{Enqueued, NewRun, QueueError};
pub use repos::NewRepo;
pub use worker::spawn_pool;
