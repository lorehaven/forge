//! The pipeline a commit declares, and what a given run makes of it.
//!
//! Nothing in here touches the network, the database or the filesystem. It
//! takes the text of a `.conveyor.toml` and a description of the run, and
//! returns what should happen - which is why it is also the part with the tests.
//!
//! A library of its own so that conveyor-service and conveyor-cli can share one
//! parser rather than two that agree most of the time. The service is a
//! deployable image and has no business in the dependency graph of a CLI
//! installed from a registry; this does, and carries none of the service's
//! weight with it.

pub mod condition;
pub mod graph;
pub mod parser;
pub mod spec;
// What a step means: the argument vector it becomes, and whether the tool it
// names would accept it. The parser calls into it, so a mistyped command is a
// parse error naming the stage at fault rather than a deploy that fails after
// the build and test stages have already spent their time.
pub mod steps;

pub use condition::{Condition, EvalContext};
pub use graph::{Decision, JobPlan, StagePlan, plan};
pub use parser::{SpecError, parse};
pub use spec::{Job, PIPELINE_FILE, PipelineSpec, Stage, Step, Triggers};
