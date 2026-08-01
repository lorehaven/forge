//! Conveyor - the CI/CD service for the Forge estate.
//!
//! A webhook arrives, conveyor checks out the commit that triggered it, reads
//! the `.conveyor.toml` that commit declares, and runs it. Pipelines are
//! versioned with the code they build, so a branch can change its own build and
//! the change is reviewable in the pull request that makes it.
//!
//! Identity is gatehouse's: conveyor verifies realm tokens locally and sends a
//! browser to gatehouse when there is no session, exactly like every other
//! relying party in the estate.

use actix_web::web;
use quench_starter::prelude::*;

pub mod artifacts;
pub mod config;
pub mod domain;
pub mod executors;
// The pipeline language is its own crate, so `conveyor validate` can link the
// parser a run will actually use without linking the service around it. Kept
// under the name it has always had here: to everything below, it is still
// `crate::pipeline`.
pub use conveyor_pipeline as pipeline;
// Turning a step into something an executor can spawn moved with the language
// it belongs to: what `anvil publish` means is a property of the pipeline, not
// of the runtime that happens to execute it.
pub use conveyor_pipeline::steps;
pub mod providers;
pub mod routers;
pub mod scan;
pub mod scheduler;
pub mod secrets;
pub mod startup;
pub mod workspace;

pub fn root_scope() -> impl HttpServiceFactory {
    routers::root_scope()
}

pub fn base_path_scope(state: startup::AppState) -> impl HttpServiceFactory {
    let jwt_config = state.jwt_config.get_ref().clone();
    state
        .install(web::scope(""))
        .service(routers::base_path_scope(jwt_config))
}
