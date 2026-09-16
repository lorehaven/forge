//! Getting from a queued run to a finished one.
//! No `concurrency` module: the per-repo rule lives in the claim query + a partial unique index, so it holds across replicas.

pub mod projects;
pub mod queue;
pub mod repos;
pub mod worker;

pub use projects::NewProject;
pub use queue::{Enqueued, NewRun, QueueError};
pub use repos::NewRepo;
pub use worker::spawn_pool;
