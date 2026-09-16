pub mod engine;
pub mod kubernetes;
pub mod launch;
pub mod list;
pub mod mock;
pub mod modals;
pub mod native;
pub mod reaper;
pub mod sse;
pub mod stop;
pub mod types;

use crate::routers::vllm::engine::{VllmEngine, VllmManagementMode};
use crate::routers::vllm::kubernetes::KubernetesVllmEngine;
use crate::routers::vllm::mock::MockVllmEngine;
use crate::routers::vllm::native::NativeVllmEngine;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_http::prelude::{Endpoint, OnPathPrefix, wrap};
use std::sync::Arc;

pub use reaper::spawn_reaper;
pub use sse::init_vllm_status_publisher;
pub use types::*;

// Initialization

pub async fn init_engine() -> Arc<dyn VllmEngine> {
    let mode = VllmManagementMode::from_env();
    tracing::info!("Initializing vLLM management in {:?} mode", mode);

    match mode {
        VllmManagementMode::Native => Arc::new(NativeVllmEngine),
        VllmManagementMode::Mock => Arc::new(MockVllmEngine),
        VllmManagementMode::Kubernetes => match KubernetesVllmEngine::new().await {
            Ok(e) => Arc::new(e),
            Err(err) => {
                tracing::error!(
                    "Failed to initialize Kubernetes vLLM engine: {}. Falling back to Native.",
                    err
                );
                Arc::new(NativeVllmEngine)
            }
        },
    }
}

// Middleware wrapping

/// No `RequireWrite`: launch/stop check `mod_impl::can` directly.
/// `base_path` matters - `OnPathPrefix` sees the raw un-mounted path.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/api/v1/vllm").into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}

pub fn register_routes() {
    let _ = list::list_instances_canonical as fn(_) -> _;
    let _ = list::list_instances_alias as fn(_) -> _;
    let _ = list::handle_grid as fn(_, _, _) -> _;
    let _ = modals::handle_launch_modal as fn(_) -> _;
    let _ = modals::empty_launch_modal as fn() -> _;
    let _ = modals::handle_stop_modal as fn(_) -> _;
    let _ = modals::empty_stop_modal as fn() -> _;
    let _ = launch::launch_instance as fn(_, _, _, _) -> _;
    let _ = launch::launch_instance_form as fn(_, _, _, _) -> _;
    let _ = stop::stop_instance as fn(_, _, _, _) -> _;
    let _ = sse::handle_sse_canonical as fn(_) -> _;
    let _ = sse::handle_sse_alias as fn(_) -> _;
}
