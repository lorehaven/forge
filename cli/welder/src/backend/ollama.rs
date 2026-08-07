use crate::backend::Backend;
use crate::config::CONFIG;
use crate::ui;
use quench_cli::require::require_binary;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct OllamaConfig {
    pub model: String,
    pub base_url: String,
}

#[derive(Debug)]
pub struct OllamaBackend {
    base_url: String,
    child: Mutex<Option<Child>>,
}

impl OllamaBackend {
    #[must_use]
    pub const fn new(base_url: String) -> Self {
        Self {
            base_url,
            child: Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl Backend for OllamaBackend {
    fn initialize(&self) -> anyhow::Result<()> {
        if self.is_running() {
            return Ok(());
        }

        require_binary(
            "ollama",
            "backend.kind = \"ollama\" needs the ollama daemon; install it from https://ollama.com or switch backend.kind",
        )?;

        let child = Command::new("ollama")
            .arg("serve")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        *self.child.lock().unwrap() = Some(child);

        Ok(())
    }

    fn initialized(&self) {
        let url = CONFIG
            .backend
            .ollama_url
            .clone()
            .expect("config error: backend.ollama_url must be set");

        ui::print_backend_banner("ollama", &url);
    }

    fn is_running(&self) -> bool {
        std::net::TcpStream::connect(&self.base_url).is_ok()
    }
}
