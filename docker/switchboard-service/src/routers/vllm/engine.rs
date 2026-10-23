use crate::routers::vllm::{LaunchRequest, VllmInstance};
use quench_core::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VllmManagementMode {
    Native,
    Kubernetes,
    Mock,
}

impl VllmManagementMode {
    #[must_use]
    pub fn from_env() -> Self {
        match quench_core::env::get_or("VLLM_MANAGEMENT_MODE", "")
            .to_lowercase()
            .as_str()
        {
            "kubernetes" | "k8s" => Self::Kubernetes,
            "mock" => Self::Mock,
            _ => Self::Native,
        }
    }
}

#[async_trait]
pub trait VllmEngine: Send + Sync {
    async fn list_instances(&self) -> Result<Vec<VllmInstance>, String>;
    async fn launch_instance(&self, req: LaunchRequest) -> Result<VllmInstance, String>;
    async fn stop_instance(&self, id: String) -> Result<(), String>;

    /// Bring back the next instance that stopped without being asked to (e.g. after a machine
    /// reboot), returning its id. Does nothing while another instance is still loading, so
    /// models start one at a time. Only the Kubernetes engine has anything to restore.
    async fn restore_next(&self) -> Result<Option<String>, String> {
        Ok(None)
    }
}
