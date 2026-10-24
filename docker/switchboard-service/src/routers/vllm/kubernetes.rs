use crate::routers::vllm::engine::VllmEngine;
use crate::routers::vllm::{
    LaunchRequest, VllmInstance, cpu_image, cpu_kvcache_space_gib, device_launch_args,
    is_cpu_device, task_launch_args,
};
use quench_core::async_trait;
use quench_core::time::Time;
use quench_k8s::{Api, ApiResource, Client, DeleteParams, ListParams, Object, PostParams};
use quench_serde::{Value, json};
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
pub fn hf_offline_env() -> quench_serde::Value {
    json!({ "name": "HF_HUB_OFFLINE", "value": "1" })
}

const MANAGED_SELECTOR: &str = "app.kubernetes.io/managed-by=switchboard,app=vllm";

/// Restarts of one model in a row before the restorer stops trying.
const MAX_RESTORES: u32 = 5;
/// A pod that served this long before stopping did not crash-loop; its restart count resets.
const STABLE_RUN_SECS: i64 = 600;
/// A pod still loading after this long no longer holds up the next restore.
const LOADING_TIMEOUT_SECS: i64 = 1200;
const RESTART_ANNOTATION: &str = "vllm-restarts";

/// Whether a pod status is one the restorer brings back.
#[must_use]
pub fn is_restorable(status: &str) -> bool {
    matches!(status, "completed" | "interrupted")
}

/// A pod that is still coming up (not yet Ready, not stuck forever).
fn is_loading(pod: &Value) -> bool {
    let age = Object(pod)
        .creation_timestamp()
        .map_or(0, |t| Time::now().timestamp() - t.timestamp());
    pod_status(pod) == "starting" && age < LOADING_TIMEOUT_SECS
}

/// The restart count the replacement pod carries: one more than the old pod's, unless it ran long
/// enough to count as stable.
fn next_restart_count(pod: &Value) -> u32 {
    let previous = Object(pod)
        .annotation(RESTART_ANNOTATION)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let ran_secs = pod
        .pointer("/status/containerStatuses/0/state/terminated")
        .and_then(|t| {
            let moment = |field: &str| {
                Time::parse_rfc3339(t.get(field)?.as_str()?)
                    .ok()
                    .map(|time| time.timestamp())
            };
            Some(moment("finishedAt")? - moment("startedAt")?)
        })
        .unwrap_or(0);
    if ran_secs >= STABLE_RUN_SECS {
        1
    } else {
        previous + 1
    }
}

fn new_pod_port(pod: &Value) -> u16 {
    Object(pod)
        .label("vllm-port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(8000)
}

/// A fresh pod with the old one's spec, labels and annotations. Scheduler- and
/// admission-injected fields are dropped so the API server can fill them in again.
///
/// # Errors
/// When the old pod has no spec to restore.
pub fn restored_pod(old: &Value, restarts: u32) -> Result<Value, String> {
    let mut spec = old
        .get("spec")
        .cloned()
        .ok_or_else(|| "Failed to serialize pod spec: the pod has none".to_string())?;
    let injected = |name: &Value| {
        name.as_str()
            .is_some_and(|n| n.starts_with("kube-api-access-"))
    };
    if let Some(obj) = spec.as_object_mut() {
        obj.remove("nodeName");
        if let Some(volumes) = obj.get_mut("volumes").and_then(|v| v.as_array_mut()) {
            volumes.retain(|v| !injected(&v["name"]));
        }
        if let Some(containers) = obj.get_mut("containers").and_then(|c| c.as_array_mut()) {
            for c in containers {
                if let Some(mounts) = c.get_mut("volumeMounts").and_then(|m| m.as_array_mut()) {
                    mounts.retain(|m| !injected(&m["name"]));
                }
            }
        }
    }
    let mut annotations: std::collections::BTreeMap<&str, &str> =
        Object(old).annotations().collect();
    let restarts = restarts.to_string();
    annotations.insert(RESTART_ANNOTATION, &restarts);
    let labels: std::collections::BTreeMap<&str, &str> = Object(old).labels().collect();
    Ok(json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": Object(old).name(),
            "labels": labels,
            "annotations": annotations,
        },
        "spec": spec,
    }))
}

/// Polls until the pod is gone (it is already terminal, so this is quick).
async fn wait_gone(pods: &Api, name: &str) -> Result<(), String> {
    for _ in 0..60 {
        if pods
            .get_opt(name)
            .await
            .map_err(|e| format!("Failed to check pod {name}: {e}"))?
            .is_none()
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    Err(format!("Pod {name} did not terminate in time"))
}

pub struct KubernetesVllmEngine {
    client: Client,
    namespace: String,
}

impl KubernetesVllmEngine {
    pub fn new() -> Result<Self, String> {
        let client = Client::infer().map_err(|e| format!("Failed to create kube client: {e}"))?;

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

/// Pod `status.reason` values that mean the pod was cut off rather than crashed.
const INTERRUPTED_REASONS: [&str; 3] = ["Shutdown", "Terminated", "UnexpectedAdmissionError"];

/// What the pod is doing, in the words the UI uses. Pod phase flips to Running before vLLM finishes
/// loading - the Ready condition (the `/health` readiness probe) is what says it is serving.
///
/// `completed` (phase `Succeeded`) and `interrupted` (`Failed` because the node shut down or the
/// pod could not be admitted) are what a machine reboot leaves behind; both are restorable, unlike
/// a genuine `Failed` crash, which the reaper removes.
#[must_use]
pub fn pod_status(pod: &Value) -> &str {
    let ready = pod
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .is_some_and(|conds| {
            conds.iter().any(|c| {
                c.get("type").and_then(Value::as_str) == Some("Ready")
                    && c.get("status").and_then(Value::as_str) == Some("True")
            })
        });

    if Object(pod).deletion_timestamp().is_some() {
        return "terminating";
    }
    match pod.pointer("/status/phase").and_then(Value::as_str) {
        Some("Running") if ready => "running",
        Some("Running" | "Pending") => "starting",
        Some("Succeeded") => "completed",
        Some("Failed")
            if pod
                .pointer("/status/reason")
                .and_then(Value::as_str)
                .is_some_and(|r| INTERRUPTED_REASONS.contains(&r)) =>
        {
            "interrupted"
        }
        Some(p) => p,
        None => "unknown",
    }
}

/// An annotation's value, unless it is absent or empty.
fn annotation(pod: &Value, key: &str) -> Option<String> {
    Object(pod)
        .annotation(key)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// The instance a managed pod stands for, read back from its labels and annotations.
fn instance_from_pod(pod: &Value) -> VllmInstance {
    let meta = Object(pod);
    let name = meta.name().unwrap_or_default().to_string();
    let namespace = meta.namespace().unwrap_or("default").to_string();

    let model = meta
        .annotation("vllm-model")
        .map(str::to_string)
        .or_else(|| meta.label("vllm-model").map(|m| m.replace("--", "/")))
        .unwrap_or_default();
    let port = meta
        .label("vllm-port")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8000);
    let parsed = |key: &str| meta.annotation(key);
    let flag = |key: &str| meta.annotation(key) == Some("true");

    VllmInstance {
        id: format!("pod:{namespace}:{name}"),
        host: format!("{name}.{namespace}.svc.cluster.local"),
        namespace,
        model,
        port,
        quantization: annotation(pod, "vllm-quantization"),
        dtype: annotation(pod, "vllm-dtype"),
        limit_mm_per_prompt: annotation(pod, "vllm-limit-mm-per-prompt"),
        max_model_len: parsed("vllm-max-model-len").and_then(|v| v.parse::<u32>().ok()),
        gpu_memory_utilization: parsed("vllm-gpu-memory-utilization")
            .and_then(|v| v.parse::<f32>().ok()),
        enable_prefix_caching: flag("vllm-enable-prefix-caching"),
        enable_tool_calling: flag("vllm-enable-tool-calling"),
        task: annotation(pod, "vllm-task"),
        device: annotation(pod, "vllm-device"),
        started_at: meta.creation_timestamp().unwrap_or_else(Time::now),
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
        quench_log::info!(
            "Enabled native tool calling for model {} (--enable-auto-tool-choice --tool-call-parser hermes)",
            req.model
        );
    }

    if let Some(ref task) = req.task {
        let task_args = task_launch_args(task);
        quench_log::info!(
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
    volume_mounts: Vec<quench_serde::Value>,
    volumes: Vec<quench_serde::Value>,
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
fn launch_env(cpu: bool, is_amd: bool) -> Vec<quench_serde::Value> {
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
    volume_mounts: &[quench_serde::Value],
) -> quench_serde::Value {
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
    container: &quench_serde::Value,
    volumes: &[quench_serde::Value],
) -> quench_serde::Value {
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
fn service_manifest(pod_name: &str, pod_uid: &str, port: u16) -> quench_serde::Value {
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

impl KubernetesVllmEngine {
    fn pods(&self, namespace: &str) -> Api {
        self.client.namespaced(&ApiResource::pod(), namespace)
    }

    fn services(&self, namespace: &str) -> Api {
        self.client.namespaced(&ApiResource::service(), namespace)
    }
}

#[async_trait]
impl VllmEngine for KubernetesVllmEngine {
    async fn list_instances(&self) -> Result<Vec<VllmInstance>, String> {
        let pods = self.client.api(&ApiResource::pod());
        let lp = ListParams::default().labels(MANAGED_SELECTOR);
        let pod_list = pods
            .list(&lp)
            .await
            .map_err(|e| format!("Failed to list pods: {e}"))?;

        Ok(pod_list.items.iter().map(instance_from_pod).collect())
    }

    async fn launch_instance(&self, req: LaunchRequest) -> Result<VllmInstance, String> {
        let namespace = req.namespace.as_deref().unwrap_or(&self.namespace);
        let pods = self.pods(namespace);

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

        let pp = PostParams::default();
        let pod = pods
            .create(&pod_manifest(&req, &pod_name, &container, &volumes), &pp)
            .await
            .map_err(|e| format!("Failed to create pod in namespace {namespace}: {e}"))?;

        let pod_uid = Object(&pod).uid().unwrap_or_default();
        self.services(namespace)
            .create(&service_manifest(&pod_name, pod_uid, req.port), &pp)
            .await
            .map_err(|e| format!("Failed to create service in namespace {namespace}: {e}"))?;

        let name = Object(&pod).name().unwrap_or_default();
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
            started_at: Time::now(),
            status: "starting".to_string(),
            log_path: None,
            last_error: None,
        })
    }

    async fn restore_next(&self) -> Result<Option<String>, String> {
        let pods = self.client.api(&ApiResource::pod());
        let lp = ListParams::default().labels(MANAGED_SELECTOR);
        let mut list = pods
            .list(&lp)
            .await
            .map_err(|e| format!("Failed to list pods: {e}"))?
            .items;
        list.sort_by(|a, b| Object(a).name().cmp(&Object(b).name()));

        // One at a time: the GPU cannot hold two models loading at once.
        if list.iter().any(is_loading) {
            return Ok(None);
        }
        let Some(old) = list.iter().find(|p| is_restorable(pod_status(p))) else {
            return Ok(None);
        };

        let name = Object(old).name().unwrap_or_default().to_string();
        let namespace = Object(old).namespace().unwrap_or_default().to_string();
        let restarts = next_restart_count(old);
        if restarts > MAX_RESTORES {
            // Leave it visible as `completed` rather than looping on a model that never stays up.
            quench_log::error!(
                "vLLM restorer: {namespace}/{name} keeps stopping; giving up after {MAX_RESTORES} restarts"
            );
            return Ok(None);
        }

        let new_pod = restored_pod(old, restarts)?;
        let port = new_pod_port(old);

        let pods = self.pods(&namespace);
        let services = self.services(&namespace);
        let _ = services.delete(&name, &DeleteParams::default()).await;
        pods.delete(&name, &DeleteParams::default())
            .await
            .map_err(|e| format!("Failed to delete pod {name}: {e}"))?;
        wait_gone(&pods, &name).await?;

        let created = pods
            .create(&new_pod, &PostParams::default())
            .await
            .map_err(|e| format!("Failed to recreate pod {name}: {e}"))?;
        let uid = Object(&created).uid().unwrap_or_default();
        let svc = service_manifest(&name, uid, port);
        // The old service may still be terminating; retry briefly rather than leave the pod unreachable.
        for _ in 0..10 {
            match services.create(&svc, &PostParams::default()).await {
                Ok(_) => break,
                Err(e) if e.is_conflict() => {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                Err(e) => return Err(format!("Failed to recreate service {name}: {e}")),
            }
        }
        Ok(Some(format!("pod:{namespace}:{name}")))
    }

    async fn stop_instance(&self, id: String) -> Result<(), String> {
        let (namespace, name) = parse_instance_id(&id, &self.namespace)?;

        let dp = DeleteParams::default();
        self.pods(&namespace)
            .delete(&name, &dp)
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
