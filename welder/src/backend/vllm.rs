use crate::backend::Backend;
use crate::ui;

#[derive(Debug)]
pub struct VllmBackend;

impl Default for VllmBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl VllmBackend {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Backend for VllmBackend {
    fn initialize(&self) -> anyhow::Result<()> {
        // vLLM instances are managed by ModelManager based on workflow configuration
        Ok(())
    }

    fn initialized(&self) {
        ui::print_backend_banner("vllm", "(managed by workflow)");
    }

    fn is_running(&self) -> bool {
        true
    }
}
