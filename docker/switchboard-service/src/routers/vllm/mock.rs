use crate::routers::vllm::engine::VllmEngine;
use crate::routers::vllm::{LaunchRequest, VllmInstance};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

static MOCK_INSTANCES: LazyLock<RwLock<HashMap<String, VllmInstance>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub struct MockVllmEngine;

#[async_trait]
impl VllmEngine for MockVllmEngine {
    async fn list_instances(&self) -> Result<Vec<VllmInstance>, String> {
        Ok(MOCK_INSTANCES.read().unwrap().values().cloned().collect())
    }

    async fn launch_instance(&self, req: LaunchRequest) -> Result<VllmInstance, String> {
        let id = format!("mock-{}", Utc::now().timestamp_millis());
        let instance = VllmInstance {
            id: id.clone(),
            namespace: req.namespace.unwrap_or_else(|| "default".to_string()),
            model: req.model,
            host: req.host,
            port: req.port,
            quantization: req.quantization,
            dtype: req.dtype,
            max_model_len: req.max_model_len,
            gpu_memory_utilization: req.gpu_memory_utilization,
            enable_prefix_caching: req.enable_prefix_caching,
            enable_tool_calling: req.enable_tool_calling,
            task: req.task,
            started_at: Utc::now(),
            status: "running".to_string(),
            log_path: None,
            last_error: None,
        };

        MOCK_INSTANCES.write().unwrap().insert(id, instance.clone());
        Ok(instance)
    }

    async fn stop_instance(&self, id: String) -> Result<(), String> {
        if MOCK_INSTANCES.write().unwrap().remove(&id).is_some() {
            Ok(())
        } else {
            Err(format!("Instance {id} not found"))
        }
    }
}
