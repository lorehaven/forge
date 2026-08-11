use crate::config::CONFIG;
use std::sync::{Arc, OnceLock};

pub mod ollama;
pub mod switchboard;
pub mod vllm;

#[async_trait::async_trait]
pub trait Backend: Send + Sync {
    fn initialize(&self) -> anyhow::Result<()>;
    fn initialized(&self);
    fn is_running(&self) -> bool;
}

static BACKEND: OnceLock<Arc<dyn Backend>> = OnceLock::new();

/// Builds and initializes the configured backend. A bad or incomplete
/// `[backend]` config is a startup error to report cleanly, not a panic - the
/// caller runs this once, early, and propagates the result.
pub fn init() -> anyhow::Result<()> {
    let backend: Arc<dyn Backend> = match CONFIG.backend.kind.as_str() {
        "ollama" => {
            let url =
                CONFIG.backend.ollama_url.clone().ok_or_else(|| {
                    anyhow::anyhow!("config error: backend.ollama_url must be set")
                })?;
            let back = ollama::OllamaBackend::new(url);
            back.initialize()?;
            Arc::new(back)
        }
        "vllm" => {
            let back = vllm::VllmBackend::new();
            back.initialize()?;
            Arc::new(back)
        }
        "switchboard" => {
            let url = CONFIG.backend.switchboard_url.clone().ok_or_else(|| {
                anyhow::anyhow!("config error: backend.switchboard_url must be set")
            })?;
            let back = switchboard::SwitchboardBackend::new(url);
            back.initialize()?;
            Arc::new(back)
        }
        other => anyhow::bail!("unsupported backend: {other}"),
    };

    // `init` runs exactly once from `init_runtime`, before anything else in
    // this module is touched - a second call is a programming error, not a
    // runtime condition to recover from.
    BACKEND
        .set(backend)
        .map_err(|_| anyhow::anyhow!("backend::init called more than once"))
}

/// Panics if called before [`init`] has succeeded - the same contract
/// `LazyLock::force` used to enforce implicitly, made explicit now that
/// initialization can fail and report why.
pub fn get() -> &'static Arc<dyn Backend> {
    BACKEND
        .get()
        .expect("backend::get called before backend::init")
}
