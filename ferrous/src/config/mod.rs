#[allow(clippy::module_inception)]
pub mod config;
pub mod prompt;

pub use config::{Config, ModelBackend, ModelRole, SamplingConfig, load, print_loaded};
pub use prompt::{PromptManager, get_default_context};
