use crate::config::CONFIG;
use std::sync::{Arc, LazyLock};

pub mod ollama;
pub mod switchboard;
pub mod vllm;

#[async_trait::async_trait]
pub trait Backend: Send + Sync {
    fn initialize(&self) -> anyhow::Result<()>;
    fn initialized(&self);
    fn is_running(&self) -> bool;
}

pub static BACKEND: LazyLock<Arc<dyn Backend>> =
    LazyLock::new(|| match CONFIG.backend.kind.as_str() {
        "ollama" => {
            let back = ollama::OllamaBackend::new(
                CONFIG
                    .backend
                    .ollama_url
                    .clone()
                    .expect("config error: backend.ollama_url must be set"),
            );
            back.initialize()
                .expect("error: failed to initialize backend");
            Arc::new(back)
        }
        "vllm" => {
            let back = vllm::VllmBackend::new();
            back.initialize()
                .expect("error: failed to initialize vllm backend");
            Arc::new(back)
        }
        "switchboard" => {
            let url = CONFIG
                .backend
                .switchboard_url
                .clone()
                .expect("config error: backend.switchboard_url must be set");
            let back = switchboard::SwitchboardBackend::new(url);
            back.initialize()
                .expect("error: failed to initialize switchboard backend");
            Arc::new(back)
        }
        _ => panic!("unsupported backend: {}", CONFIG.backend.kind),
    });
