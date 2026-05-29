use crate::routers::vllm::{LaunchRequest, VllmInstance};
use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VllmManagementMode {
    Native,
    Kubernetes,
}

impl VllmManagementMode {
    pub fn from_env() -> Self {
        match envmnt::get_or("VLLM_MANAGEMENT_MODE", "")
            .to_lowercase()
            .as_str()
        {
            "kubernetes" | "k8s" => Self::Kubernetes,
            _ => Self::Native,
        }
    }
}

#[async_trait]
pub trait VllmEngine: Send + Sync {
    async fn list_instances(&self) -> Result<Vec<VllmInstance>, String>;
    async fn launch_instance(&self, req: LaunchRequest) -> Result<VllmInstance, String>;
    async fn stop_instance(&self, id: String) -> Result<(), String>;
}
