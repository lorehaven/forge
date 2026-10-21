use crate::routers::vllm::engine::VllmEngine;
use crate::routers::vllm::{
    LaunchRequest, VllmInstance, cpu_image, cpu_kvcache_space_gib, device_launch_args,
    is_cpu_device, task_launch_args,
};
use async_trait::async_trait;
use chrono::Utc;
use k8s_openapi::api::core::v1::{Pod, Service};
use k8s_openapi::jiff;
use kube::Client;
use kube::api::{Api, DeleteParams, ListParams, PostParams};
use serde_json::json;
use std::hash::{Hash, Hasher};

/// Creates valid Kubernetes names for vLLM model instances.
///
/// Kubernetes Service names (and label values, which the pod also carries this string as)
/// are capped at 63 characters and must be a valid RFC 1035 label. Long model names -
/// especially ones with an org prefix - routinely blow past that once "vllm-" and the port
/// are added, so a name that doesn't fit is truncated with a short content hash appended
/// for uniqueness rather than sent to the API and rejected.
#[must_use]
pub fn vllm_pod_name(model: &str, port: u16) -> String {
    const MAX_LEN: usize = 63;
    const PREFIX: &str = "vllm-";

    let safe_model = model.replace(['/', '.'], "-").to_lowercase();
    let suffix = format!("-{port}");
    let full = format!("{PREFIX}{safe_model}{suffix}");
    if full.len() <= MAX_LEN {
        return full;
    }

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    safe_model.hash(&mut hasher);
    let hash = format!(
        "{:08x}",
        u32::try_from(hasher.finish() & u64::from(u32::MAX)).expect("masked hash fits in u32")
    );

    // -1 for the hyphen separating the truncated model from the hash.
    let budget = MAX_LEN.saturating_sub(PREFIX.len() + suffix.len() + hash.len() + 1);
    let mut truncated: String = safe_model.chars().take(budget).collect();
    while truncated.ends_with('-') {
        truncated.pop();
    }

    format!("{PREFIX}{truncated}-{hash}{suffix}")
}

/// Env var that keeps `huggingface_hub/transformers/vLLM` on the mounted cache.
///
/// Switchboard only launches models already on disk, but vLLM still asks the Hub whether
/// optional files the snapshot lacks (tokenizer.model, `added_tokens.json`,
/// model.safetensors.index.json, ...) exist. For a gated repo that unauthenticated request
/// is a 401 rather than a 404, which aborts the launch. Offline mode skips those lookups.
#[must_use]
pub fn hf_offline_env() -> serde_json::Value {
    json!({ "name": "HF_HUB_OFFLINE", "value": "1" })
}

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

/// What the pod is doing, in the words the UI uses. Pod phase flips to Running before vLLM finishes
/// loading - the Ready condition (the `/health` readiness probe) is what says it is serving.
fn pod_status(pod: &Pod) -> &str {
    let ready = pod
        .status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .is_some_and(|conds| {
            conds
                .iter()
                .any(|c| c.type_ == "Ready" && c.status == "True")
        });

    if pod.metadata.deletion_timestamp.is_some() {
        return "terminating";
    }
    match pod.status.as_ref().and_then(|s| s.phase.as_deref()) {
        Some("Running") if ready => "running",
        Some("Running" | "Pending") => "starting",
        Some(p) => p,
        None => "unknown",
    }
}

/// An annotation's value, unless it is absent or empty.
fn annotation(
    annotations: &std::collections::BTreeMap<String, String>,
    key: &str,
) -> Option<String> {
    annotations.get(key).filter(|v| !v.is_empty()).cloned()
}

/// The instance a managed pod stands for, read back from its labels and annotations.
fn instance_from_pod(pod: &Pod) -> VllmInstance {
    let name = pod.metadata.name.clone().unwrap_or_default();
    let namespace = pod
        .metadata
        .namespace
        .clone()
        .unwrap_or_else(|| "default".to_string());
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

    let started_at = pod
        .metadata
        .creation_timestamp
        .as_ref()
        .map_or_else(jiff::Timestamp::now, |t| t.0);

    VllmInstance {
        id: format!("pod:{namespace}:{name}"),
        host: format!("{name}.{namespace}.svc.cluster.local"),
        namespace,
        model,
        port,
        quantization: annotation(&annotations, "vllm-quantization"),
        dtype: annotation(&annotations, "vllm-dtype"),
        limit_mm_per_prompt: annotation(&annotations, "vllm-limit-mm-per-prompt"),
        max_model_len: annotations
            .get("vllm-max-model-len")
            .and_then(|v| v.parse::<u32>().ok()),
        gpu_memory_utilization: annotations
            .get("vllm-gpu-memory-utilization")
            .and_then(|v| v.parse::<f32>().ok()),
        enable_prefix_caching: annotations
            .get("vllm-enable-prefix-caching")
            .is_some_and(|v| v == "true"),
        enable_tool_calling: annotations
            .get("vllm-enable-tool-calling")
            .is_some_and(|v| v == "true"),
        task: annotation(&annotations, "vllm-task"),
        device: annotation(&annotations, "vllm-device"),
        started_at: chrono::DateTime::<Utc>::from_timestamp(
            started_at.as_second(),
            started_at.subsec_nanosecond().cast_unsigned(),
        )
        .expect("timestamp out of range for chrono"),
        status: pod_status(pod).to_string(),
        log_path: None, // K8s logs are accessed differently
        last_error: None,
    }
}

/// The container's arguments for a launch.
fn launch_args(req: &LaunchRequest, cpu: bool) -> Vec<String> {
    // CPU entrypoint takes the model positionally; GPU's api_server wants `--model`.
    let mut args = if cpu {
        vec![req.model.clone()]
    } else {
        vec!["--model".to_string(), req.model.clone()]
    };
    args.extend([
        "--host".to_string(),
        "0.0.0.0".to_string(),
        "--port".to_string(),
        req.port.to_string(),
    ]);

    if let Some(ref q) = req.quantization {
        args.push("--quantization".to_string());
        args.push(q.clone());
    }

    if let Some(ref dtype) = req.dtype {
        args.push("--dtype".to_string());
        args.push(dtype.clone());
    }

    if let Some(ref device) = req.device {
        args.extend(device_launch_args(device));
    }

    if let Some(ref limit) = req.limit_mm_per_prompt {
        args.push("--limit-mm-per-prompt".to_string());
        args.push(limit.clone());
    }

    if let Some(seqs) = req.max_num_seqs {
        args.push("--max-num-seqs".to_string());
        args.push(seqs.to_string());
    }

    if let Some(len) = req.max_model_len {
        args.push("--max-model-len".to_string());
        args.push(len.to_string());
    }

    if let Some(util) = req.gpu_memory_utilization
        && !cpu
    {
        args.push("--gpu-memory-utilization".to_string());
        args.push(format!("{util:.2}"));
    }

    if req.enable_prefix_caching {
        args.push("--enable-prefix-caching".to_string());
    }

    if req.enable_tool_calling {
        args.push("--enable-auto-tool-choice".to_string());
        args.push("--tool-call-parser".to_string());
        args.push("hermes".to_string());
        tracing::info!(
            "Enabled native tool calling for model {} (--enable-auto-tool-choice --tool-call-parser hermes)",
            req.model
        );
    }

    if let Some(ref task) = req.task {
        let task_args = task_launch_args(task);
        tracing::info!(
            "Launching model {} for task {} ({})",
            req.model,
            task,
            task_args.join(" ")
        );
        args.extend(task_args);
    }
    args
}

/// The volume mounts and the volumes behind them, as `(volumeMounts, volumes)`.
struct Mounts {
    volume_mounts: Vec<serde_json::Value>,
    volumes: Vec<serde_json::Value>,
}

impl Mounts {
    /// A read-write `hostPath` of `kind`, mounted at `mount_path`.
    fn host_path(&mut self, name: &str, mount_path: &str, path: &str, kind: &str) {
        self.volume_mounts
            .push(json!({ "name": name, "mountPath": mount_path }));
        self.volumes
            .push(json!({ "name": name, "hostPath": { "path": path, "type": kind } }));
    }

    /// A read-only `hostPath` directory.
    fn read_only_dir(&mut self, name: &str, mount_path: &str, path: &str, kind: &str) {
        self.volume_mounts
            .push(json!({ "name": name, "mountPath": mount_path, "readOnly": true }));
        self.volumes
            .push(json!({ "name": name, "hostPath": { "path": path, "type": kind } }));
    }
}

/// An env var's value, or `default` when it is unset.
fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// The mounts a launch needs: accelerator devices and the venv for the GPU path, the HF cache for
/// both, and every model root read-only.
fn launch_mounts(cpu: bool, is_amd: bool) -> Mounts {
    let mut mounts = Mounts {
        volume_mounts: Vec::new(),
        volumes: Vec::new(),
    };

    // GPU accelerator device nodes - never for a CPU launch.
    if !cpu {
        mounts.host_path("kfd", "/dev/kfd", "/dev/kfd", "CharDevice");
        mounts.host_path("dri", "/dev/dri", "/dev/dri", "Directory");
    }

    // The venv and ROCm mounts for the custom image.
    if is_amd && !cpu {
        let venv_path = env_or("VLLM_VENV_PATH", "/mnt/dev/vllm/vllm-venv");
        let rocm_path = env_or("VLLM_ROCM_PATH", "/opt/rocm");
        mounts.read_only_dir("venv", "/opt/venv", &venv_path, "Directory");
        mounts.read_only_dir("rocm", "/opt/rocm", &rocm_path, "Directory");
        let vllm_cache_path = env_or("VLLM_CACHE_PATH", "/mnt/dev/vllm/cache");
        mounts.host_path(
            "vllm-cache",
            "/root/.cache/vllm",
            &vllm_cache_path,
            "DirectoryOrCreate",
        );
    }

    // Both paths need the HF cache; the CPU image needs none of the venv/ROCm mounts.
    let hf_cache_path = env_or("HF_CACHE_PATH", "/mnt/dev/huggingface/cache");
    mounts.host_path(
        "hf-cache",
        "/root/.cache/huggingface",
        &hf_cache_path,
        "DirectoryOrCreate",
    );

    // Mount model directories
    let hf_roots = env_or("HF_ROOTS", "/mnt/dev/huggingface/hub");
    let gguf_roots = env_or("GGUF_ROOTS", "/mnt/dev/quantized");
    let all_roots = hf_roots
        .split(':')
        .chain(gguf_roots.split(':'))
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .collect::<std::collections::HashSet<_>>();
    for (i, root) in all_roots.into_iter().enumerate() {
        mounts.read_only_dir(
            &format!("model-root-{i}"),
            &root,
            &root,
            "DirectoryOrCreate",
        );
    }
    mounts
}

/// The container's env: offline HF, plus the CPU or `ROCm` settings.
fn launch_env(cpu: bool, is_amd: bool) -> Vec<serde_json::Value> {
    let mut env_vars = vec![hf_offline_env()];

    if cpu {
        env_vars.push(json!({
            "name": "VLLM_CPU_KVCACHE_SPACE",
            "value": cpu_kvcache_space_gib()
        }));
        if let Ok(bind) = std::env::var("VLLM_CPU_OMP_THREADS_BIND") {
            env_vars.push(json!({
                "name": "VLLM_CPU_OMP_THREADS_BIND",
                "value": bind
            }));
        }
    }

    // Add AMD-specific environment variables if using AMD
    if is_amd && !cpu {
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
    env_vars
}

/// The image to run: the CPU image, or vLLM for the GPU vendor in use.
fn launch_image(cpu: bool, is_amd: bool) -> String {
    if cpu {
        return cpu_image();
    }
    std::env::var("VLLM_IMAGE").unwrap_or_else(|_| {
        if is_amd {
            "rocm/vllm:latest".to_string()
        } else {
            "vllm/vllm-openai:latest".to_string()
        }
    })
}

/// The pod's single container.
fn container_manifest(
    req: &LaunchRequest,
    cpu: bool,
    is_amd: bool,
    gpu_resource_key: &str,
    volume_mounts: &[serde_json::Value],
) -> serde_json::Value {
    // A CPU launch must not request a GPU device plugin resource, or the
    // pod stays Pending on hosts with no allocatable GPUs.
    let key = gpu_resource_key.trim();
    let resources = if !cpu && !key.is_empty() && key.to_lowercase() != "none" {
        json!({ "limits": { key: "1" } })
    } else {
        json!({})
    };

    let mut container = json!({
        "name": "vllm",
        "image": launch_image(cpu, is_amd),
        "imagePullPolicy": "Always",
        "args": launch_args(req, cpu),
        "ports": [
            {
                "containerPort": req.port,
                "name": "http"
            }
        ],
        "resources": resources,
        "env": launch_env(cpu, is_amd),
        "volumeMounts": volume_mounts,
        // vLLM serves /health once the model is loaded; until then
        // the pod stays NotReady so list_instances reports "starting".
        "readinessProbe": {
            "httpGet": {
                "path": "/health",
                "port": req.port
            },
            "initialDelaySeconds": 5,
            "periodSeconds": 5,
            "timeoutSeconds": 3
        }
    });

    if cpu {
        // CPU image's own entrypoint needs no GPU devices; SYS_NICE is enough.
        container["securityContext"] = json!({
            "capabilities": { "add": ["SYS_NICE"] }
        });
    } else {
        // GPU path: run vLLM from the host-mounted venv, privileged for the
        // ROCm device nodes.
        container["command"] = json!([
            "/opt/venv/bin/python",
            "-m",
            "vllm.entrypoints.openai.api_server",
        ]);
        container["securityContext"] = json!({ "privileged": true });
    }
    container
}

/// The pod for a launch; its annotations are what `instance_from_pod` reads back.
fn pod_manifest(
    req: &LaunchRequest,
    pod_name: &str,
    container: &serde_json::Value,
    volumes: &[serde_json::Value],
) -> serde_json::Value {
    json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": pod_name,
            "labels": {
                "app": "vllm",
                "app.kubernetes.io/managed-by": "switchboard",
                "vllm-model": req.model.replace('/', "--"), // Labels have restrictions
                "vllm-port": req.port.to_string(),
                "vllm-pod-name": pod_name,
            },
            "annotations": {
                "vllm-model": req.model.clone(),
                "vllm-quantization": req.quantization.as_deref().unwrap_or(""),
                "vllm-dtype": req.dtype.as_deref().unwrap_or(""),
                "vllm-limit-mm-per-prompt": req.limit_mm_per_prompt.as_deref().unwrap_or(""),
                "vllm-max-model-len": req.max_model_len.map(|v| v.to_string()).unwrap_or_default(),
                "vllm-gpu-memory-utilization": req.gpu_memory_utilization.map(|v| v.to_string()).unwrap_or_default(),
                "vllm-enable-prefix-caching": req.enable_prefix_caching.to_string(),
                "vllm-enable-tool-calling": req.enable_tool_calling.to_string(),
                "vllm-task": req.task.as_deref().unwrap_or(""),
                "vllm-device": req.device.as_deref().unwrap_or(""),
            }
        },
        "spec": {
            "hostIPC": true,
            "containers": [container],
            "volumes": volumes,
            "restartPolicy": "Never"
        }
    })
}

/// A `ClusterIP` service for internal access, owned by the pod so it goes when the pod does.
fn service_manifest(pod_name: &str, pod_uid: &str, port: u16) -> serde_json::Value {
    json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {
            "name": pod_name,
            "labels": {
                "app": "vllm",
                "app.kubernetes.io/managed-by": "switchboard",
            },
            "ownerReferences": [
                {
                    "apiVersion": "v1",
                    "kind": "Pod",
                    "name": pod_name,
                    "uid": pod_uid,
                    "controller": true,
                    "blockOwnerDeletion": true
                }
            ]
        },
        "spec": {
            "selector": {
                "vllm-pod-name": pod_name
            },
            "ports": [
                {
                    "port": port,
                    "targetPort": port,
                    "name": "http"
                }
            ],
            "type": "ClusterIP"
        }
    })
}

/// The namespace and pod name an instance id names.
fn parse_instance_id(id: &str, default_namespace: &str) -> Result<(String, String), String> {
    if let Some(rest) = id.strip_prefix("pod:") {
        return rest
            .split_once(':')
            .map(|(namespace, name)| (namespace.to_string(), name.to_string()))
            .ok_or_else(|| format!("Invalid instance id format: {id}"));
    }
    // Fallback for old ID format
    id.strip_prefix("pod-")
        .map(|name| (default_namespace.to_string(), name.to_string()))
        .ok_or_else(|| format!("Invalid instance id for kubernetes: {id}"))
}

#[async_trait]
impl VllmEngine for KubernetesVllmEngine {
    async fn list_instances(&self) -> Result<Vec<VllmInstance>, String> {
        let pods: Api<Pod> = Api::all(self.client.clone());
        let lp = ListParams::default().labels("app.kubernetes.io/managed-by=switchboard,app=vllm");
        let pod_list = pods
            .list(&lp)
            .await
            .map_err(|e| format!("Failed to list pods: {e}"))?;

        Ok(pod_list.iter().map(instance_from_pod).collect())
    }

    async fn launch_instance(&self, req: LaunchRequest) -> Result<VllmInstance, String> {
        let namespace = req.namespace.as_deref().unwrap_or(&self.namespace);
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), namespace);

        let gpu_resource_key = env_or("VLLM_K8S_GPU_RESOURCE", "amd.com/gpu");
        let is_amd = gpu_resource_key.to_lowercase().contains("amd")
            || gpu_resource_key.to_lowercase() == "none";

        // CPU launches run a dedicated image that ships its own vLLM + entrypoint;
        // GPU launches run vLLM from a host-mounted venv (the `is_amd` path).
        let cpu = is_cpu_device(req.device.as_deref());

        let pod_name = vllm_pod_name(&req.model, req.port);
        let Mounts {
            volume_mounts,
            volumes,
        } = launch_mounts(cpu, is_amd);
        let container = container_manifest(&req, cpu, is_amd, &gpu_resource_key, &volume_mounts);

        let p: Pod = serde_json::from_value(pod_manifest(&req, &pod_name, &container, &volumes))
            .map_err(|e| format!("Failed to deserialize pod manifest: {e}"))?;

        let pp = PostParams::default();
        let pod = pods
            .create(&pp, &p)
            .await
            .map_err(|e| format!("Failed to create pod in namespace {namespace}: {e}"))?;

        let pod_uid = pod.metadata.uid.as_deref().unwrap_or_default();
        let s: Service = serde_json::from_value(service_manifest(&pod_name, pod_uid, req.port))
            .map_err(|e| format!("Failed to deserialize service manifest: {e}"))?;

        let services: Api<Service> = Api::namespaced(self.client.clone(), namespace);
        services
            .create(&pp, &s)
            .await
            .map_err(|e| format!("Failed to create service in namespace {namespace}: {e}"))?;

        let name = pod.metadata.name.clone().unwrap_or_default();
        Ok(VllmInstance {
            id: format!("pod:{namespace}:{name}"),
            namespace: namespace.to_string(),
            model: req.model.clone(),
            host: format!("{name}.{namespace}.svc.cluster.local"),
            port: req.port,
            quantization: req.quantization.clone(),
            dtype: req.dtype.clone(),
            limit_mm_per_prompt: req.limit_mm_per_prompt.clone(),
            max_model_len: req.max_model_len,
            gpu_memory_utilization: req.gpu_memory_utilization,
            enable_prefix_caching: req.enable_prefix_caching,
            enable_tool_calling: req.enable_tool_calling,
            task: req.task.clone(),
            device: req.device.clone(),
            started_at: Utc::now(),
            status: "starting".to_string(),
            log_path: None,
            last_error: None,
        })
    }

    async fn stop_instance(&self, id: String) -> Result<(), String> {
        let (namespace, name) = parse_instance_id(&id, &self.namespace)?;

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &namespace);
        let dp = DeleteParams::default();
        pods.delete(&name, &dp)
            .await
            .map_err(|e| format!("Failed to delete pod {name} in namespace {namespace}: {e}"))?;

        Ok(())
    }
}

impl std::fmt::Debug for KubernetesVllmEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("KubernetesVllmEngine { .. }")
    }
}
