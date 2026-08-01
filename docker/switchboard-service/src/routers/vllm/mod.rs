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
use actix_web::dev::HttpServiceFactory;
use actix_web::web;
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::prelude::JwtConfig;
use std::sync::Arc;

pub use reaper::spawn_reaper;
pub use sse::init_vllm_status_publisher;
pub use types::*;

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// No `RequireWrite`: switchboard's catalog entry declares `"launch"` and
/// `"stop"`, not `"write"`, so a coarse write grant could never satisfy it -
/// `launch_instance(_form)` and `stop_instance` each check `mod_impl::can`
/// for their own specific action instead. Everything else here is GET,
/// including the modal renders, which only *offer* a write and never perform
/// one, so they need no check at all.
pub fn scope(engine: Arc<dyn VllmEngine>, jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("/api/v1/vllm")
        .app_data(web::Data::new(engine))
        .wrap(Auth::new(jwt_config))
        .service(list::list_instances_canonical)
        .service(list::list_instances_alias)
        .service(list::handle_grid)
        .service(modals::handle_launch_modal)
        .service(modals::empty_launch_modal)
        .service(modals::handle_stop_modal)
        .service(modals::empty_stop_modal)
        .service(launch::launch_instance)
        .service(launch::launch_instance_form)
        .service(stop::stop_instance)
        .service(sse::handle_sse_canonical)
        .service(sse::handle_sse_alias)
}
