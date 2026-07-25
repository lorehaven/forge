pub mod default_models;
pub mod state;
pub mod validate;

pub use state::AppState;
pub use validate::validate_startup;

pub fn init_tracing() {
    quench_starter::logging::init();
    tracing::info!("Sage service starting");
}

/// Readiness endpoint sage waits on before serving traffic.
pub fn health_check_url() -> String {
    let switchboard_url = envmnt::get_or("SWITCHBOARD_URL", "http://switchboard-service:8080");
    format!("{}/health/ready", switchboard_url.trim_end_matches('/'))
}
