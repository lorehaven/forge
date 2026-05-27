use crate::steps::common::WarehouseWorld;
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

    // 1. Storage directory
    let storage_dir = std::path::Path::new(manifest_dir).join("storage");
    if storage_dir.exists() {
        println!("Cleaning up storage directory: {:?}", storage_dir);
        if let Err(e) = std::fs::remove_dir_all(&storage_dir) {
            eprintln!("Warning: failed to remove storage directory: {}", e);
        }
    }
    std::fs::create_dir_all(&storage_dir).ok();

    // 2. Start warehouse-service
    println!("Starting warehouse-service...");
    let warehouse_service_dir = std::path::Path::new(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("docker")
        .join("warehouse-service");
    let mut child = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("warehouse-service")
        .current_dir(&warehouse_service_dir)
        .envs(env::vars())
        .env("SERVER_ADDR", "127.0.0.1:8443")
        .env("SERVER_HTTP_REDIRECT_ADDR", "127.0.0.1:8080")
        .env("SERVER_CERT_PATH", "cert.pem")
        .env("SERVER_KEY_PATH", "key.pem")
        .env("CRATES_STORAGE_PATH", storage_dir.join("crates"))
        .env("STORAGE_PATH", storage_dir.join("docker"))
        .spawn()
        .expect("Failed to start warehouse-service");

    // Wait for service to be ready (simple sleep for now, could be improved with health check)
    println!("Waiting for service to start...");
    sleep(Duration::from_secs(10)).await;

    // 3. Run tests
    let features_path = std::path::Path::new(manifest_dir).join("features");

    // We use catch_unwind or just run and then kill
    WarehouseWorld::run(features_path).await;

    // 4. Stop service
    println!("Stopping warehouse-service...");
    let _ = child.kill().await;
}
