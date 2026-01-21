use anyhow::{Result, anyhow};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio::time::{Duration, interval};

pub async fn is_port_open(host: &str, port: u16) -> bool {
    TcpStream::connect((host.to_string(), port)).await.is_ok()
}

/// Spawns llama-server and waits until it's ready (HTTP endpoint responds)
pub async fn spawn_server(model: &str, port: u16, debug: bool) -> Result<Arc<Mutex<Child>>> {
    let port_str = port.to_string();
    let mut args_vec = vec![
        "-m",
        model,
        "--device",
        "Vulkan0",
        "--port",
        &port_str,
        "--host",
        "127.0.0.1",
        "-c",
        "8192",
        "--temp",
        "0",
        "--repeat-penalty",
        "1.1",
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

    if debug {
        args_vec.push("--verbose");
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
    loop {
        println!(
            "Waiting for llama-server to start... (attempt {}/120)",
            attempts + 1
        );
        interv.tick().await;
        attempts += 1;

        if is_port_open("127.0.0.1", port).await {
            println!("Server ready on port {}", port);
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
    println!("Reusing existing llama-server on port {}", port);
    // We don't need to return anything special here – just confirm port is open
    if !is_port_open("127.0.0.1", port).await {
        return Err(anyhow!("No server listening on port {}", port));
    }
    Ok(())
}
