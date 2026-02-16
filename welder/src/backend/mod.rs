use crate::config::CONFIG;
use std::sync::{Arc, LazyLock};

pub mod ollama;

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
        _ => panic!("unsupported backend"),
    });
