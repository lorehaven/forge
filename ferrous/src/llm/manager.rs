use crate::config::{ModelBackend, ModelRole, SamplingConfig};
use crate::llm::{is_server_ready, spawn_server};
use anyhow::Result;
use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct ModelHandle {
    pub backend: ModelBackend,
    pub server_process: Option<Arc<Mutex<Child>>>,
    pub port: u16,
}

#[derive(Debug, Default)]
pub struct ModelManager {
    handles: HashMap<ModelRole, ModelHandle>,
}

impl ModelManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_or_start_model(
        &mut self,
        role: ModelRole,
        backend: &ModelBackend,
        sampling: &SamplingConfig,
        debug: bool,
    ) -> Result<ModelHandle> {
        if let Some(handle) = self.handles.get(&role) {
            return Ok(handle.clone());
        }

        let handle = match backend {
            ModelBackend::LocalLlama {
                model_path,
                port,
                context_size,
                num_gpu_layers,
            } => {
                let port = *port;
                let server_process = if is_server_ready(port).await {
                    None
                } else {
                    Some(
                        spawn_server(
                            model_path,
                            *context_size,
                            sampling.temperature.unwrap_or(0.2),
                            sampling.repeat_penalty.unwrap_or(1.1),
                            port,
                            debug,
                            None,
                        )
                        .await?,
                    )
                };
                ModelHandle {
                    backend: ModelBackend::LocalLlama {
                        model_path: model_path.clone(),
                        port,
                        context_size: *context_size,
                        num_gpu_layers: *num_gpu_layers,
                    },
                    server_process,
                    port,
                }
            }
            ModelBackend::OpenAi { .. }
            | ModelBackend::Anthropic { .. }
            | ModelBackend::External { .. } => {
                // For remote backends, we don't spawn a server
                ModelHandle {
                    backend: backend.clone(),
                    server_process: None,
                    port: 0, // Not used for remote
                }
            }
        };

        self.handles.insert(role, handle.clone());
        Ok(handle)
    }

    #[must_use]
    pub fn get_handle(&self, role: ModelRole) -> Option<&ModelHandle> {
        self.handles.get(&role)
    }
}
