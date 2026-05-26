use actix_web::dev::HttpServiceFactory;
use actix_web::{HttpResponse, Responder, delete, get, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, RwLock};
use std::time::Duration;
use utoipa::{OpenApi, ToSchema};

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    paths(list_instances, launch_instance, stop_instance, handle_ws),
    components(schemas(VllmInstance, LaunchRequest))
)]
pub struct VllmApiDoc;

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct VllmInstance {
    pub id: String,
    pub model: String,
    pub host: String,
    pub port: u16,
    pub quantization: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,

    #[schema(value_type = String, format = DateTime)]
    pub started_at: DateTime<Utc>,

    pub status: String,
    pub log_path: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LaunchRequest {
    pub model: String,
    pub host: String,
    pub port: u16,
    pub quantization: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,
}

#[derive(Debug, Clone)]
struct LaunchRecord {
    pid: u32,
    log_path: Option<String>,
    last_error: Option<String>,
}

static LAUNCH_RECORDS: LazyLock<RwLock<HashMap<String, LaunchRecord>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn discover_vllm_instances() -> Vec<VllmInstance> {
    let mut instances = Vec::new();
    let records = LAUNCH_RECORDS.read().unwrap().clone();

    let proc_entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(_) => return instances,
    };

    for entry in proc_entries.flatten() {
        let pid = match entry.file_name().to_string_lossy().parse::<u32>() {
            Ok(pid) => pid,
            Err(_) => continue,
        };

        let cmdline_path = format!("/proc/{pid}/cmdline");

        let cmdline_raw = match fs::read(&cmdline_path) {
            Ok(data) => data,
            Err(_) => continue,
        };

        let parts: Vec<String> = cmdline_raw
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).to_string())
            .collect();

        if parts.len() < 3 {
            continue;
        }

        let joined = parts.join(" ");

        if !joined.contains("vllm serve") {
            continue;
        }

        let model = match parts.iter().position(|p| p == "serve") {
            Some(idx) if idx + 1 < parts.len() => parts[idx + 1].clone(),
            _ => continue,
        };

        let host = extract_arg(&parts, "--host").unwrap_or_else(|| "0.0.0.0".to_string());

        let port = extract_arg(&parts, "--port")
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8000);

        let quantization = extract_arg(&parts, "--quantization");

        let max_model_len =
            extract_arg(&parts, "--max-model-len").and_then(|v| v.parse::<u32>().ok());

        let gpu_memory_utilization =
            extract_arg(&parts, "--gpu-memory-utilization").and_then(|v| v.parse::<f32>().ok());

        let enable_prefix_caching = parts.iter().any(|p| p == "--enable-prefix-caching");

        let started_at = process_started_at(pid).unwrap_or_else(Utc::now);

        let record_key = instance_key(&model, port);
        let record = records.get(&record_key);
        let status = match record {
            Some(r) if r.pid == pid => match r.log_path.as_deref() {
                Some(path) if log_indicates_started(path, r.pid) => "running",
                Some(_) => "starting",
                None => "starting",
            },
            Some(_) => "running",
            None => "running",
        };

        instances.push(VllmInstance {
            id: format!("pid-{pid}"),
            model,
            host,
            port,
            quantization,
            max_model_len,
            gpu_memory_utilization,
            enable_prefix_caching,
            started_at,
            status: status.to_string(),
            log_path: record.and_then(|r| r.log_path.clone()),
            last_error: record.and_then(|r| r.last_error.clone()),
        });
    }

    instances
}

fn extract_arg(parts: &[String], key: &str) -> Option<String> {
    parts
        .windows(2)
        .find(|window| window[0] == key)
        .map(|window| window[1].clone())
}

fn process_started_at(pid: u32) -> Option<DateTime<Utc>> {
    let metadata = fs::metadata(format!("/proc/{pid}")).ok()?;
    let created = metadata.modified().ok()?;

    Some(created.into())
}

fn instance_key(model: &str, port: u16) -> String {
    format!("{}-{}", model.replace("/", "--"), port)
}

fn find_available_port(host: &str, requested_port: u16) -> Result<u16, String> {
    for port in requested_port..=u16::MAX {
        match TcpListener::bind((host, port)) {
            Ok(listener) => {
                drop(listener);
                return Ok(port);
            }
            Err(_) => continue,
        }
    }

    Err(format!(
        "no free port available starting from {requested_port}"
    ))
}

fn create_launch_log_path(model: &str, port: u16) -> String {
    let log_dir = std::env::var("VLLM_LOG_DIR").unwrap_or_else(|_| "dist/vllm_logs".to_string());
    let _ = fs::create_dir_all(&log_dir);
    let safe_model = model.replace('/', "__");
    format!(
        "{}/{}-{}-{}.log",
        log_dir,
        Utc::now().format("%Y%m%dT%H%M%S"),
        safe_model,
        port
    )
}

fn create_pid_log_path(base_log_path: &str, pid: u32) -> String {
    match base_log_path.strip_suffix(".log") {
        Some(prefix) => format!("{prefix}-pid-{pid}.log"),
        None => format!("{base_log_path}-pid-{pid}"),
    }
}

fn log_indicates_started(path: &str, pid: u32) -> bool {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(_) => return false,
    };

    contents.contains(&format!("Started server process [{pid}]"))
}

fn read_log_tail(path: &str, max_lines: usize) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    let lines = contents.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);
    Some(lines[start..].join("\n"))
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/api/v1/vllm")
        .service(list_instances)
        .service(launch_instance)
        .service(stop_instance)
        .service(handle_ws)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/instances",
    responses((status = 200, body = [VllmInstance]))
)]
#[get("/instances")]
async fn list_instances() -> impl Responder {
    let list = discover_vllm_instances();
    HttpResponse::Ok().json(list)
}

#[utoipa::path(
    post,
    path = "/instances",
    request_body = LaunchRequest,
    responses((status = 202, body = VllmInstance))
)]
#[post("/instances")]
async fn launch_instance(req: web::Json<LaunchRequest>) -> impl Responder {
    let port = match find_available_port(&req.host, req.port) {
        Ok(port) => port,
        Err(err) => return HttpResponse::BadRequest().body(err),
    };
    let id = instance_key(&req.model, port);
    let log_path = create_launch_log_path(&req.model, port);

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

    let stdout_file = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(file) => file,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .body(format!("Failed to open vllm log file {log_path}: {e}"));
        }
    };
    let stderr_file = match stdout_file.try_clone() {
        Ok(file) => file,
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!(
                "Failed to clone vllm log file handle {log_path}: {e}"
            ));
        }
    };

    let mut command = Command::new("vllm");
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));

    #[cfg(unix)]
    command.process_group(0);

    match command.spawn() {
        Ok(mut child) => {
            let pid = child.id();
            let pid_log_path = create_pid_log_path(&log_path, pid);
            let log_path = match fs::rename(&log_path, &pid_log_path) {
                Ok(_) => pid_log_path,
                Err(_) => log_path,
            };
            let started_at = Utc::now();
            LAUNCH_RECORDS.write().unwrap().insert(
                id.clone(),
                LaunchRecord {
                    pid,
                    log_path: Some(log_path.clone()),
                    last_error: None,
                },
            );

            std::thread::sleep(Duration::from_millis(1200));

            if let Ok(Some(status)) = child.try_wait() {
                let log_tail = read_log_tail(&log_path, 40);
                LAUNCH_RECORDS.write().unwrap().insert(
                    id.clone(),
                    LaunchRecord {
                        pid,
                        log_path: Some(log_path.clone()),
                        last_error: log_tail.clone(),
                    },
                );

                let message = match log_tail {
                    Some(log_tail) if !log_tail.is_empty() => format!(
                        "vLLM exited during startup with status {status}. Log: {log_path}\n\n{log_tail}"
                    ),
                    _ => {
                        format!("vLLM exited during startup with status {status}. Log: {log_path}")
                    }
                };
                return HttpResponse::InternalServerError().body(message);
            }

            LAUNCH_RECORDS.write().unwrap().insert(
                id.clone(),
                LaunchRecord {
                    pid,
                    log_path: Some(log_path.clone()),
                    last_error: None,
                },
            );

            let instance = VllmInstance {
                id: format!("pid-{pid}"),
                model: req.model.clone(),
                host: req.host.clone(),
                port,
                quantization: req.quantization.clone(),
                max_model_len: req.max_model_len,
                gpu_memory_utilization: req.gpu_memory_utilization,
                enable_prefix_caching: req.enable_prefix_caching,
                started_at,
                status: "starting".to_string(),
                log_path: Some(log_path),
                last_error: None,
            };

            drop(child);

            HttpResponse::Accepted().json(instance)
        }
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed to spawn vllm: {e}")),
    }
}

#[utoipa::path(
    delete,
    path = "/instances/{id}",
    responses((status = 204))
)]
#[delete("/instances/{id}")]
async fn stop_instance(id: web::Path<String>) -> impl Responder {
    let pid = match id.strip_prefix("pid-").and_then(|v| v.parse::<i32>().ok()) {
        Some(pid) => pid,
        None => return HttpResponse::BadRequest().finish(),
    };

    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status();

    match status {
        Ok(status) if status.success() => HttpResponse::NoContent().finish(),
        _ => HttpResponse::InternalServerError().finish(),
    }
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/instances/ws",
    responses(
        (status = 101, description = "WebSocket upgraded"),
    )
)]
#[get("/instances/ws")]
async fn handle_ws(
    req: actix_web::HttpRequest,
    body: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let (response, session, stream) = actix_ws::handle(&req, body)?;

    let session_clone = session.clone();

    actix_web::rt::spawn(async move {
        websocket_sender(session).await;
    });

    actix_web::rt::spawn(async move {
        websocket_receiver(stream, session_clone).await;
    });

    Ok(response)
}

async fn websocket_sender(mut session: actix_ws::Session) {
    let mut interval = actix_web::rt::time::interval(std::time::Duration::from_secs(1));

    loop {
        interval.tick().await;

        let list = discover_vllm_instances();

        match serde_json::to_string(&list) {
            Ok(json) => {
                if session.text(json).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                tracing::error!("serialize vllm instances failed: {e}");
            }
        }
    }
}

async fn websocket_receiver(mut stream: actix_ws::MessageStream, mut session: actix_ws::Session) {
    use futures_util::StreamExt;

    while let Some(item) = stream.next().await {
        match item {
            Ok(actix_ws::Message::Ping(bytes)) if session.pong(&bytes).await.is_err() => {
                break;
            }

            Ok(actix_ws::Message::Close(reason)) => {
                let _ = session.close(reason).await;
                break;
            }

            _ => {}
        }
    }
}
