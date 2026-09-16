//! Conveyor - the CI/CD service for the Forge estate.
//! A webhook triggers a checkout, reads `.conveyor.toml`, and runs it; identity is gatehouse's.

// Router `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]

pub mod artifacts;
pub mod config;
pub mod credentials;
pub mod domain;
pub mod executors;
// Own crate so `conveyor validate` can link the parser without the service.
pub use conveyor_pipeline as pipeline;
pub use conveyor_pipeline::steps;
pub mod providers;
pub mod routers;
pub mod scan;
pub mod scheduler;
pub mod secrets;
pub mod startup;
pub mod workspace;
