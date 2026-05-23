use crate::steps::common::SwitchboardWorld;
use cucumber::World;
use std::env;
use tokio::process::Command;
use tokio::time::{Duration, sleep};

mod steps;

#[tokio::main]
async fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let env_path = std::path::Path::new(manifest_dir).join(".env");
    dotenvy::from_path(env_path).ok();

    // 1. Start switchboard-service
    println!("Starting switchboard-service...");
    let switchboard_service_dir = std::path::Path::new(manifest_dir)
        .parent()
        .unwrap()
        .join("switchboard-service");
    let mut child = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("switchboard-service")
        .current_dir(&switchboard_service_dir)
        .envs(env::vars())
        .env("SERVER_ADDR", "127.0.0.1:8554")
        .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:8081")
        .env("SERVER_CERT_PATH", "cert.pem")
        .env("SERVER_KEY_PATH", "key.pem")
        .spawn()
        .expect("Failed to start switchboard-service");

    // Wait for service to be ready
    println!("Waiting for service to start...");
    sleep(Duration::from_secs(10)).await;

    // 2. Run tests
    let features_path = std::path::Path::new(manifest_dir).join("features");

    SwitchboardWorld::run(features_path).await;

    // 3. Stop service
    println!("Stopping switchboard-service...");
    let _ = child.kill().await;
}
