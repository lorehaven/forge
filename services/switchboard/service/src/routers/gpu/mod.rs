use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_http::prelude::{Endpoint, Json, OnPathPrefix, get, wrap};
use std::sync::Arc;

pub mod monitor;

pub use monitor::get_gpu_info;

/// Wraps `/api/v1/gpu` in `Auth`. `base_path` matters: `OnPathPrefix` sees
/// the raw un-mounted path, which still carries `BASE_PATH`.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/api/v1/gpu").into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}

pub fn register_routes() {
    let _ = get_status as fn() -> _;
}

// REST endpoint

#[get("/api/v1/gpu/status")]
pub async fn get_status() -> Json<monitor::GpuInfo> {
    Json(get_gpu_info().unwrap_or_default())
}
