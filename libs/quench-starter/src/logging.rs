/// Initialize logging with configurable verbosity via RUST_LOG environment variable
pub fn init() {
    dotenvy::dotenv().ok();

    let log_level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(tracing::Level::INFO);

    tracing_subscriber::fmt().with_max_level(log_level).init();

    let actual_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    tracing::debug!("Logging initialized with RUST_LOG={}", actual_level);
}
