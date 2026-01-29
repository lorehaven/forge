use anyhow::{Result, anyhow};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio::time::{Duration, interval};

#[derive(Debug, Clone, Copy)]
pub enum ModelLoadPhase {
    StartingServer,
    WaitingForPort,
    Ready,
}

pub type ProgressCallback = Box<dyn Fn(ModelLoadPhase) + Send>;

pub async fn is_port_open(host: &str, port: u16) -> bool {
    TcpStream::connect((host.to_string(), port)).await.is_ok()
}

/// Spawns llama-server and waits until it's ready (HTTP endpoint responds)
pub async fn spawn_server(
    model: &str,
    context: u32,
    temperature: f32,
    repeat_penalty: f32,
    port: u16,
    debug: bool,
    progress: Option<ProgressCallback>,
) -> Result<Arc<Mutex<Child>>> {
    let port_str = port.to_string();
    let context_str = context.to_string();
    let temperature_str = temperature.to_string();
    let repeat_penalty_str = repeat_penalty.to_string();
    let args_vec = vec![
        "-m",
        model,
        "--device",
        "Vulkan0",
        "--port",
        &port_str,
        "--host",
        "127.0.0.1",
        "-c",
        &context_str,
        "--temp",
        &temperature_str,
        "--repeat-penalty",
        &repeat_penalty_str,
        "--jinja",
        "--flash-attn",
        "auto",
        "-ngl",
        "999",
        "-t",
        "12",
        "--mlock",
        "--no-mmap",
    ];

    if let Some(cb) = &progress {
        cb(ModelLoadPhase::StartingServer);
    }

    let mut child = Command::new("llama-server")
        .env("GGML_VULKAN_DEVICE", "0")
        .args(&args_vec)
        .stdout(if debug {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .stderr(if debug {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .spawn()?;

    let mut interv = interval(Duration::from_secs(2));
    let mut attempts = 0;

    if let Some(cb) = &progress {
        cb(ModelLoadPhase::WaitingForPort);
    }

    loop {
        println!(
            "Waiting for llama-server to start... (attempt {}/120)",
            attempts + 1
        );
        interv.tick().await;
        attempts += 1;

        if is_port_open("127.0.0.1", port).await {
            if let Some(cb) = &progress {
                cb(ModelLoadPhase::Ready);
            }

            println!("Server ready on port {port}");
            break;
        }

        if attempts >= 120 {
            let _ = child.kill();
            return Err(anyhow!("Server failed to become ready after 4 minutes"));
        }
    }

    Ok(Arc::new(Mutex::new(child)))
}

/// Connect to an already running server (no spawn)
pub async fn connect_only(port: u16) -> Result<()> {
    println!("Reusing existing llama-server on port {port}");
    if !is_port_open("127.0.0.1", port).await {
        return Err(anyhow!("No server listening on port {port}"));
    }
    Ok(())
}
