#[allow(clippy::module_inception)]
pub mod config;
pub mod prompt;

pub use config::{
    Config, ModelBackend, ModelRole, SamplingConfig, UiMode, UiTheme, load, print_loaded, save,
};
pub use prompt::{PromptManager, get_default_context};
