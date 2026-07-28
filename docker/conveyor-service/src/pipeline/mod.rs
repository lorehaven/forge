//! The pipeline a commit declares, and what a given run makes of it.
//!
//! Nothing in here touches the network, the database or the filesystem. It
//! takes the text of a `.conveyor.toml` and a description of the run, and
//! returns what should happen - which is why it is also the part with the tests.

pub mod condition;
pub mod graph;
pub mod parser;
pub mod spec;

pub use condition::{Condition, EvalContext};
pub use graph::{Decision, JobPlan, StagePlan, plan};
pub use parser::{SpecError, parse};
pub use spec::{Job, PIPELINE_FILE, PipelineSpec, Stage, Step, Triggers};
