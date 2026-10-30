use crate::routers::vllm::engine::VllmEngine;
use crate::routers::vllm::{
    LaunchRequest, VllmInstance, cpu_kvcache_space_gib, device_from_args, device_launch_args,
    is_cpu_device, task_from_args, task_launch_args,
};
use quench_core::async_trait;
use quench_core::time::Time;
use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, RwLock};
use std::time::Duration;

#[derive(Debug, Clone)]
struct LaunchRecord {
    pid: u32,
    model: String,
    host: String,
    port: u16,
    quantization: Option<String>,
    dtype: Option<String>,
    limit_mm_per_prompt: Option<String>,
    max_model_len: Option<u32>,
    gpu_memory_utilization: Option<f32>,
    enable_prefix_caching: bool,
    enable_tool_calling: bool,
    task: Option<String>,
    device: Option<String>,
    started_at: Time,
    terminating_at: Option<Time>,
    log_path: Option<String>,
    last_error: Option<String>,
}

static LAUNCH_RECORDS: LazyLock<RwLock<HashMap<String, LaunchRecord>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Debug)]
pub struct NativeVllmEngine;

/// What a `vllm serve` process's command line says about the instance, joined with the record of
/// its launch (when this process launched it). `None` for a process that is not a `vllm serve`.
fn instance_from_process(
    pid: u32,
    parts: &[String],
    records: &HashMap<String, LaunchRecord>,
) -> Option<VllmInstance> {
    if parts.len() < 3 || !parts.join(" ").contains("vllm serve") {
        return None;
    }

    let model = match parts.iter().position(|p| p == "serve") {
        Some(idx) if idx + 1 < parts.len() => parts[idx + 1].clone(),
        _ => return None,
    };
    let host = extract_arg(parts, "--host").unwrap_or_else(|| "0.0.0.0".to_string());
    let port = extract_arg(parts, "--port")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8000);

    let record = records.get(&instance_key(&model, port));
    let terminating = |r: &LaunchRecord| r.last_error.as_deref() == Some("terminating");
    let status = match record {
        Some(r) if r.pid == pid => {
            if terminating(r) {
                "terminating"
            } else {
                match r.log_path.as_deref() {
                    Some(path) if log_indicates_started(path, r.pid) => "running",
                    Some(_) | None => "starting",
                }
            }
        }
        Some(_) | None => "running",
    };

    Some(VllmInstance {
        id: format!("pid-{pid}"),
        namespace: "native".to_string(),
        model,
        host,
        port,
        quantization: extract_arg(parts, "--quantization"),
        dtype: extract_arg(parts, "--dtype"),
        limit_mm_per_prompt: extract_arg(parts, "--limit-mm-per-prompt"),
        max_model_len: extract_arg(parts, "--max-model-len").and_then(|v| v.parse::<u32>().ok()),
        gpu_memory_utilization: extract_arg(parts, "--gpu-memory-utilization")
            .and_then(|v| v.parse::<f32>().ok()),
        enable_prefix_caching: parts.iter().any(|p| p == "--enable-prefix-caching"),
        enable_tool_calling: parts.iter().any(|p| p == "--enable-auto-tool-choice"),
        task: task_from_args(parts),
        device: device_from_args(parts),
        started_at: process_started_at(pid).unwrap_or_else(Time::now),
        status: status.to_string(),
        log_path: record.and_then(|r| r.log_path.clone()),
        last_error: record.and_then(|r| {
            if terminating(r) {
                None
            } else {
                r.last_error.clone()
            }
        }),
    })
}

impl LaunchRecord {
    /// A record being stopped whose process has not (yet) vanished from the listing.
    fn is_terminating(&self) -> bool {
        self.last_error.as_deref() == Some("terminating")
    }

    /// The instance as the listing shows it while it is stopping.
    fn as_terminating_instance(&self) -> VllmInstance {
        VllmInstance {
            id: format!("pid-{}", self.pid),
            namespace: "native".to_string(),
            model: self.model.clone(),
            host: self.host.clone(),
            port: self.port,
            quantization: self.quantization.clone(),
            dtype: self.dtype.clone(),
            limit_mm_per_prompt: self.limit_mm_per_prompt.clone(),
            max_model_len: self.max_model_len,
            gpu_memory_utilization: self.gpu_memory_utilization,
            enable_prefix_caching: self.enable_prefix_caching,
            enable_tool_calling: self.enable_tool_calling,
            task: self.task.clone(),
            device: self.device.clone(),
            started_at: self.started_at,
            status: "terminating".to_string(),
            log_path: self.log_path.clone(),
            last_error: None,
        }
    }
}

/// What a launch started, to be recorded under its instance key and shown in the listing.
struct Launched<'a> {
    req: &'a LaunchRequest,
    pid: u32,
    port: u16,
    started_at: Time,
    log_path: &'a str,
}

impl Launched<'_> {
    fn record(&self, last_error: Option<String>) -> LaunchRecord {
        let req = self.req;
        LaunchRecord {
            pid: self.pid,
            model: req.model.clone(),
            host: req.host.clone(),
            port: self.port,
            quantization: req.quantization.clone(),
            dtype: req.dtype.clone(),
            limit_mm_per_prompt: req.limit_mm_per_prompt.clone(),
            max_model_len: req.max_model_len,
            gpu_memory_utilization: req.gpu_memory_utilization,
            enable_prefix_caching: req.enable_prefix_caching,
            enable_tool_calling: req.enable_tool_calling,
            task: req.task.clone(),
            device: req.device.clone(),
            started_at: self.started_at,
            terminating_at: None,
            log_path: Some(self.log_path.to_string()),
            last_error,
        }
    }

    fn instance(&self) -> VllmInstance {
        let req = self.req;
        VllmInstance {
            id: format!("pid-{}", self.pid),
            namespace: "native".to_string(),
            model: req.model.clone(),
            host: req.host.clone(),
            port: self.port,
            quantization: req.quantization.clone(),
            dtype: req.dtype.clone(),
            limit_mm_per_prompt: req.limit_mm_per_prompt.clone(),
            max_model_len: req.max_model_len,
            gpu_memory_utilization: req.gpu_memory_utilization,
            enable_prefix_caching: req.enable_prefix_caching,
            enable_tool_calling: req.enable_tool_calling,
            task: req.task.clone(),
            device: req.device.clone(),
            started_at: self.started_at,
            status: "starting".to_string(),
            log_path: Some(self.log_path.to_string()),
            last_error: None,
        }
    }
}

/// The arguments for `vllm serve`.
fn serve_args(req: &LaunchRequest, port: u16, cpu: bool) -> Vec<String> {
    let mut args = vec![
        "serve".to_string(),
        req.model.clone(),
        "--host".to_string(),
        req.host.clone(),
        "--port".to_string(),
        port.to_string(),
    ];

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

    // `--gpu-memory-utilization` is meaningless on CPU; the CPU backend
    // sizes its KV cache from VLLM_CPU_KVCACHE_SPACE instead (set at spawn).
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

/// The `vllm` command for a launch, writing both streams to `log_path`.
fn serve_command(
    req: &LaunchRequest,
    port: u16,
    cpu: bool,
    log_path: &str,
) -> Result<Command, String> {
    let stdout_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| format!("Failed to open vllm log file {log_path}: {e}"))?;
    let stderr_file = stdout_file
        .try_clone()
        .map_err(|e| format!("Failed to clone vllm log file handle {log_path}: {e}"))?;

    let mut command = Command::new("vllm");
    command
        .args(serve_args(req, port, cpu))
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));

    if cpu {
        let kvcache = cpu_kvcache_space_gib();
        command.env("VLLM_CPU_KVCACHE_SPACE", &kvcache);
        quench_log::info!(
            "Launching model {} on CPU (--device cpu, VLLM_CPU_KVCACHE_SPACE={} GiB). \
             Requires a CPU-capable vLLM build; FP8 checkpoints won't load on CPU.",
            req.model,
            kvcache
        );
    }

    #[cfg(unix)]
    command.process_group(0);
    Ok(command)
}

#[async_trait]
impl VllmEngine for NativeVllmEngine {
    async fn list_instances(&self) -> Result<Vec<VllmInstance>, String> {
        let mut instances = Vec::new();
        let records = LAUNCH_RECORDS.read().unwrap().clone();
        let mut seen_pids = std::collections::HashSet::new();

        let Ok(proc_entries) = fs::read_dir("/proc") else {
            return Ok(instances);
        };

        for entry in proc_entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(cmdline_raw) = fs::read(format!("/proc/{pid}/cmdline")) else {
                continue;
            };
            let parts: Vec<String> = cmdline_raw
                .split(|b| *b == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).to_string())
                .collect();

            if let Some(instance) = instance_from_process(pid, &parts, &records) {
                seen_pids.insert(pid);
                instances.push(instance);
            }
        }

        // A stopped instance lingers in the listing, as terminating, for a few seconds.
        instances.extend(
            records
                .values()
                .filter(|record| {
                    record.is_terminating()
                        && !seen_pids.contains(&record.pid)
                        && record
                            .terminating_at
                            .is_some_and(|at| (Time::now() - at).num_seconds() < 15)
                })
                .map(LaunchRecord::as_terminating_instance),
        );

        LAUNCH_RECORDS.write().unwrap().retain(|_, record| {
            !record.is_terminating()
                || record
                    .terminating_at
                    .is_none_or(|at| (Time::now() - at).num_seconds() < 15)
        });

        Ok(instances)
    }

    async fn launch_instance(&self, req: LaunchRequest) -> Result<VllmInstance, String> {
        let port = find_available_port(&req.host, req.port)?;
        let id = instance_key(&req.model, port);
        let log_path = create_launch_log_path(&req.model, port);
        let cpu = is_cpu_device(req.device.as_deref());

        let mut command = serve_command(&req, port, cpu, &log_path)?;
        let mut child = command
            .spawn()
            .map_err(|e| format!("Failed to spawn vllm: {e}"))?;

        let pid = child.id();
        let pid_log_path = create_pid_log_path(&log_path, pid);
        let log_path = match fs::rename(&log_path, &pid_log_path) {
            Ok(()) => pid_log_path,
            Err(_) => log_path,
        };
        let launched = Launched {
            req: &req,
            pid,
            port,
            started_at: Time::now(),
            log_path: &log_path,
        };
        LAUNCH_RECORDS
            .write()
            .unwrap()
            .insert(id.clone(), launched.record(None));

        tokio::time::sleep(Duration::from_millis(1200)).await;

        if let Ok(Some(status)) = child.try_wait() {
            let log_tail = read_log_tail(&log_path, 40);
            LAUNCH_RECORDS
                .write()
                .unwrap()
                .insert(id, launched.record(log_tail.clone()));

            let message = match log_tail {
                Some(log_tail) if !log_tail.is_empty() => format!(
                    "vLLM exited during startup with status {status}. Log: {log_path}\n\n{log_tail}"
                ),
                _ => format!("vLLM exited during startup with status {status}. Log: {log_path}"),
            };
            return Err(message);
        }

        LAUNCH_RECORDS
            .write()
            .unwrap()
            .insert(id, launched.record(None));
        drop(child);

        Ok(launched.instance())
    }

    async fn stop_instance(&self, id: String) -> Result<(), String> {
        let Some(pid) = id.strip_prefix("pid-").and_then(|v| v.parse::<i32>().ok()) else {
            return Err(format!("Invalid instance id: {id}"));
        };

        // Mark as terminating in records
        {
            let mut records = LAUNCH_RECORDS.write().unwrap();
            for record in records.values_mut() {
                if record.pid == pid.cast_unsigned() {
                    record.last_error = Some("terminating".to_string());
                    record.terminating_at = Some(Time::now());
                }
            }
        }

        let status = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status();

        match status {
            Ok(status) if status.success() => Ok(()),
            _ => Err(format!("Failed to kill process {pid}")),
        }
    }
}

// Helpers

#[must_use]
pub fn extract_arg(parts: &[String], key: &str) -> Option<String> {
    parts
        .windows(2)
        .find(|window| window[0] == key)
        .map(|window| window[1].clone())
}

fn process_started_at(pid: u32) -> Option<Time> {
    let metadata = fs::metadata(format!("/proc/{pid}")).ok()?;
    let created = metadata.modified().ok()?;

    Some(created.into())
}

#[must_use]
pub fn instance_key(model: &str, port: u16) -> String {
    format!("{}-{}", model.replace('/', "--"), port)
}

/// Ports claimed by non-terminating records - a fresh vLLM takes seconds to
/// bind, so an OS check alone can report a port free twice over.
fn claimed_ports() -> std::collections::HashSet<u16> {
    LAUNCH_RECORDS
        .read()
        .unwrap()
        .values()
        .filter(|r| r.terminating_at.is_none())
        .map(|r| r.port)
        .collect()
}

fn find_available_port(host: &str, requested_port: u16) -> Result<u16, String> {
    let claimed = claimed_ports();
    for port in requested_port..=u16::MAX {
        if claimed.contains(&port) {
            continue;
        }
        if let Ok(listener) = TcpListener::bind((host, port)) {
            drop(listener);
            return Ok(port);
        }
    }

    Err(format!(
        "no free port available starting from {requested_port}"
    ))
}

#[must_use]
pub fn create_launch_log_path(model: &str, port: u16) -> String {
    let log_dir = std::env::var("VLLM_LOG_DIR").unwrap_or_else(|_| "dist/vllm_logs".to_string());
    let _ = fs::create_dir_all(&log_dir);
    let safe_model = model.replace('/', "__");
    format!(
        "{}/{}-{}-{}.log",
        log_dir,
        Time::now().format("%Y%m%dT%H%M%S"),
        safe_model,
        port
    )
}

#[must_use]
pub fn create_pid_log_path(base_log_path: &str, pid: u32) -> String {
    base_log_path.strip_suffix(".log").map_or_else(
        || format!("{base_log_path}-pid-{pid}"),
        |prefix| format!("{prefix}-pid-{pid}.log"),
    )
}

#[must_use]
pub fn log_indicates_started(path: &str, pid: u32) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        return false;
    };

    contents.contains(&format!("Started server process [{pid}]"))
}

#[must_use]
pub fn read_log_tail(path: &str, max_lines: usize) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    let lines = contents.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);
    Some(lines[start..].join("\n"))
}
