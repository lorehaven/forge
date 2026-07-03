/// Initialize logging with configurable verbosity via RUST_LOG environment variable
pub fn init() {
    // Try to load .env from common locations for local development
    let _ = dotenvy::dotenv(); // Load from project root

    // Try to load from service-specific .env file if it exists
    if let Ok(bin_name) = std::env::var("CARGO_BIN_NAME") {
        let service_env_path = format!("docker/{}/", bin_name);
        let env_file = std::path::Path::new(&service_env_path).join(".env");
        if env_file.exists() {
            let _ = dotenvy::from_path(&env_file);
        }
    }

    let log_level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(tracing::Level::INFO);

    tracing_subscriber::fmt().with_max_level(log_level).init();

    let actual_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing::debug!("Logging initialized with RUST_LOG={}", actual_level);
}
