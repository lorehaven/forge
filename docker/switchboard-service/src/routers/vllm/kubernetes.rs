use crate::routers::vllm::engine::VllmEngine;
use crate::routers::vllm::{LaunchRequest, VllmInstance};
use async_trait::async_trait;
use chrono::Utc;
use k8s_openapi::api::core::v1::Pod;
use kube::Client;
use kube::api::{Api, DeleteParams, ListParams, PostParams};
use serde_json::json;

pub struct KubernetesVllmEngine {
    client: Client,
    namespace: String,
}

impl KubernetesVllmEngine {
    pub async fn new() -> Result<Self, String> {
        let client = Client::try_default()
            .await
            .map_err(|e| format!("Failed to create kube client: {e}"))?;

        let namespace = std::env::var("VLLM_K8S_NAMESPACE")
            .ok()
            .or_else(|| {
                // Try to detect namespace from service account if running in-cluster
                std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
                    .ok()
            })
            .unwrap_or_else(|| "default".to_string())
            .trim()
            .to_string();

        Ok(Self { client, namespace })
    }
}

#[async_trait]
impl VllmEngine for KubernetesVllmEngine {
    async fn list_instances(&self) -> Result<Vec<VllmInstance>, String> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        let lp = ListParams::default().labels("app.kubernetes.io/managed-by=switchboard,app=vllm");
        let pod_list = pods
            .list(&lp)
            .await
            .map_err(|e| format!("Failed to list pods: {e}"))?;

        let mut instances = Vec::new();
        for pod in pod_list {
            let name = pod.metadata.name.clone().unwrap_or_default();
            let labels = pod.metadata.labels.clone().unwrap_or_default();
            let annotations = pod.metadata.annotations.clone().unwrap_or_default();

            let model = annotations
                .get("vllm-model")
                .cloned()
                .or_else(|| labels.get("vllm-model").map(|m| m.replace("--", "/")))
                .unwrap_or_default();
            let port = labels
                .get("vllm-port")
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(8000);

            let status = match pod.status.as_ref().and_then(|s| s.phase.as_deref()) {
                Some("Running") => "running",
                Some("Pending") => "starting",
                Some(p) => p,
                None => "unknown",
            };

            let started_at = pod
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0)
                .unwrap_or_else(Utc::now);

            instances.push(VllmInstance {
                id: format!("pod-{name}"),
                model,
                host: pod
                    .status
                    .as_ref()
                    .and_then(|s| s.pod_ip.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                port,
                quantization: annotations.get("vllm-quantization").cloned(),
                max_model_len: annotations
                    .get("vllm-max-model-len")
                    .and_then(|v| v.parse::<u32>().ok()),
                gpu_memory_utilization: annotations
                    .get("vllm-gpu-memory-utilization")
                    .and_then(|v| v.parse::<f32>().ok()),
                enable_prefix_caching: annotations
                    .get("vllm-enable-prefix-caching")
                    .map(|v| v == "true")
                    .unwrap_or(false),
                started_at,
                status: status.to_string(),
                log_path: None, // K8s logs are accessed differently
                last_error: None,
            });
        }

        Ok(instances)
    }

    async fn launch_instance(&self, req: LaunchRequest) -> Result<VllmInstance, String> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        let gpu_resource_key =
            std::env::var("VLLM_K8S_GPU_RESOURCE").unwrap_or_else(|_| "amd.com/gpu".to_string());

        let is_amd = gpu_resource_key.to_lowercase().contains("amd")
            || gpu_resource_key.to_lowercase() == "none";

        let image = std::env::var("VLLM_IMAGE").unwrap_or_else(|_| {
            if is_amd {
                "rocm/vllm/latest".to_string()
            } else {
                "vllm/vllm-openai:latest".to_string()
            }
        });

        let safe_model = req.model.replace(['/', '.'], "-").to_lowercase();
        let pod_name = format!("vllm-{}-{}", safe_model, req.port);

        let mut args = vec![
            "--model".to_string(),
            req.model.clone(),
            "--host".to_string(),
            "0.0.0.0".to_string(),
            "--port".to_string(),
            req.port.to_string(),
        ];

        if let Some(ref q) = req.quantization {
            args.push("--quantization".to_string());
            args.push(q.clone());
        }

        if let Some(len) = req.max_model_len {
            args.push("--max-model-len".to_string());
            args.push(len.to_string());
        }

        if let Some(util) = req.gpu_memory_utilization {
            args.push("--gpu-memory-utilization".to_string());
            args.push(format!("{:.2}", util));
        }

        if req.enable_prefix_caching {
            args.push("--enable-prefix-caching".to_string());
        }

        let mut volume_mounts = vec![
            json!({
                "mountPath": "/dev/kfd",
                "name": "kfd"
            }),
            json!({
                "mountPath": "/dev/dri",
                "name": "dri"
            }),
        ];

        let mut volumes = vec![
            json!({
                "name": "kfd",
                "hostPath": {
                    "path": "/dev/kfd",
                    "type": "CharDevice"
                }
            }),
            json!({
                "name": "dri",
                "hostPath": {
                    "path": "/dev/dri",
                    "type": "Directory"
                }
            }),
        ];

        // Add venv and rocm mounts for the custom image
        if is_amd {
            let venv_path = std::env::var("VLLM_VENV_PATH")
                .unwrap_or_else(|_| "/mnt/dev/vllm/vllm-venv".to_string());
            let rocm_path =
                std::env::var("VLLM_ROCM_PATH").unwrap_or_else(|_| "/opt/rocm".to_string());

            volume_mounts.push(json!({
                "name": "venv",
                "mountPath": "/opt/venv",
                "readOnly": true
            }));
            volumes.push(json!({
                "name": "venv",
                "hostPath": {
                    "path": venv_path,
                    "type": "Directory"
                }
            }));

            volume_mounts.push(json!({
                "name": "rocm",
                "mountPath": "/opt/rocm",
                "readOnly": true
            }));
            volumes.push(json!({
                "name": "rocm",
                "hostPath": {
                    "path": rocm_path,
                    "type": "Directory"
                }
            }));

            // Add vLLM cache mount
            let vllm_cache_path = std::env::var("VLLM_CACHE_PATH")
                .unwrap_or_else(|_| "/mnt/dev/vllm/cache".to_string());
            volume_mounts.push(json!({
                "name": "vllm-cache",
                "mountPath": "/root/.cache/vllm"
            }));
            volumes.push(json!({
                "name": "vllm-cache",
                "hostPath": {
                    "path": vllm_cache_path,
                    "type": "DirectoryOrCreate"
                }
            }));

            // Add HF cache mount
            let hf_cache_path = std::env::var("HF_CACHE_PATH")
                .unwrap_or_else(|_| "/mnt/dev/huggingface/cache".to_string());
            volume_mounts.push(json!({
                "name": "hf-cache",
                "mountPath": "/root/.cache/huggingface"
            }));
            volumes.push(json!({
                "name": "hf-cache",
                "hostPath": {
                    "path": hf_cache_path,
                    "type": "DirectoryOrCreate"
                }
            }));
        }

        let mut env_vars = Vec::new();

        let mut resources = json!({});
        let key = gpu_resource_key.trim();
        if !key.is_empty() && key.to_lowercase() != "none" {
            resources = json!({
                "limits": {
                    key: "1"
                }
            });
        }

        // Add AMD-specific environment variables if using AMD
        if is_amd {
            // Check if user has overridden the gfx version in the host environment
            if let Ok(gfx) = std::env::var("HSA_OVERRIDE_GFX_VERSION") {
                env_vars.push(json!({
                    "name": "HSA_OVERRIDE_GFX_VERSION",
                    "value": gfx
                }));
            }
            // Add other common ROCm env vars that might be helpful
            env_vars.push(json!({
                "name": "VLLM_WORKER_MULTIPROC_METHOD",
                "value": "spawn"
            }));
            env_vars.push(json!({
                "name": "VLLM_TARGET_DEVICE",
                "value": "rocm"
            }));
        }

        // Mount model directories
        let hf_roots =
            std::env::var("HF_ROOTS").unwrap_or_else(|_| "/mnt/dev/huggingface/hub".to_string());
        let gguf_roots =
            std::env::var("GGUF_ROOTS").unwrap_or_else(|_| "/mnt/dev/quantized".to_string());

        let all_roots = hf_roots
            .split(':')
            .chain(gguf_roots.split(':'))
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect::<std::collections::HashSet<_>>(); // Use HashSet to deduplicate

        for (i, root) in all_roots.into_iter().enumerate() {
            let name = format!("model-root-{}", i);
            volume_mounts.push(json!({
                "name": name,
                "mountPath": root,
                "readOnly": true
            }));
            volumes.push(json!({
                "name": name,
                "hostPath": {
                    "path": root,
                    "type": "DirectoryOrCreate"
                }
            }));
        }

        let pod_manifest = json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {
                "name": pod_name,
                "labels": {
                    "app": "vllm",
                    "app.kubernetes.io/managed-by": "switchboard",
                    "vllm-model": req.model.replace('/', "--"), // Labels have restrictions
                    "vllm-port": req.port.to_string(),
                },
                "annotations": {
                    "vllm-model": req.model.clone(),
                    "vllm-quantization": req.quantization.as_deref().unwrap_or(""),
                    "vllm-max-model-len": req.max_model_len.map(|v| v.to_string()).unwrap_or_default(),
                    "vllm-gpu-memory-utilization": req.gpu_memory_utilization.map(|v| v.to_string()).unwrap_or_default(),
                    "vllm-enable-prefix-caching": req.enable_prefix_caching.to_string(),
                }
            },
            "spec": {
                "hostIPC": true,
                "containers": [
                    {
                        "name": "vllm",
                        "image": image,
                        "imagePullPolicy": "Always",
                        "command": [
                            "/opt/venv/bin/python",
                            "-m",
                            "vllm.entrypoints.openai.api_server",
                        ],
                        "args": args,
                        "securityContext": {
                            "privileged": true
                        },
                        "ports": [
                            {
                                "containerPort": req.port,
                                "name": "http"
                            }
                        ],
                        "resources": resources,
                        "env": env_vars,
                        "volumeMounts": volume_mounts
                    }
                ],
                "volumes": volumes,
                "restartPolicy": "Never"
            }
        });

        let p: Pod = serde_json::from_value(pod_manifest)
            .map_err(|e| format!("Failed to deserialize pod manifest: {e}"))?;

        let pp = PostParams::default();
        let pod = pods
            .create(&pp, &p)
            .await
            .map_err(|e| format!("Failed to create pod: {e}"))?;

        Ok(VllmInstance {
            id: format!("pod-{}", pod.metadata.name.unwrap_or_default()),
            model: req.model.clone(),
            host: "pending".to_string(),
            port: req.port,
            quantization: req.quantization.clone(),
            max_model_len: req.max_model_len,
            gpu_memory_utilization: req.gpu_memory_utilization,
            enable_prefix_caching: req.enable_prefix_caching,
            started_at: Utc::now(),
            status: "starting".to_string(),
            log_path: None,
            last_error: None,
        })
    }

    async fn stop_instance(&self, id: String) -> Result<(), String> {
        let name = match id.strip_prefix("pod-") {
            Some(name) => name,
            None => return Err(format!("Invalid instance id for kubernetes: {id}")),
        };

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        let dp = DeleteParams::default();
        pods.delete(name, &dp)
            .await
            .map_err(|e| format!("Failed to delete pod {name}: {e}"))?;

        Ok(())
    }
}
