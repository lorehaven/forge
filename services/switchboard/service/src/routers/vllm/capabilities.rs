//! What the signed-in person may do and what the service has switched on: the client reads it once to decide which
//! screens and buttons to show (the old pages decided this on the server, per request).

use crate::routers::models::mod_impl::{OptionalClaims, can};
use crate::routers::vllm::engine::VllmManagementMode;
use crate::routers::{models_dashboard_enabled, vllm_management_enabled};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Json, get};
use quench_serde::Serialize;

// Plain yes/no answers the client reads one by one: not a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Serialize)]
pub struct Capabilities {
    /// The models registry is on (`FEATURE_MODELS_DASHBOARD_ENABLED`).
    pub models_dashboard: bool,
    /// The vLLM management screen is on (`FEATURE_VLLM_MANAGEMENT_ENABLED`).
    pub vllm_management: bool,
    pub can_launch: bool,
    pub can_stop: bool,
    pub can_delete_model: bool,
    /// Where instances run (`native`, `mock`, or the Kubernetes namespace): the launch form's namespace.
    pub namespace: String,
}

/// The namespace instances are launched into: `native`/`mock` for those engines, the pod's namespace on Kubernetes.
#[must_use]
pub fn get_vllm_namespace() -> String {
    match VllmManagementMode::from_env() {
        VllmManagementMode::Kubernetes => std::env::var("VLLM_K8S_NAMESPACE")
            .ok()
            .or_else(|| {
                std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
                    .ok()
            })
            .unwrap_or_else(|| "default".to_string())
            .trim()
            .to_string(),
        VllmManagementMode::Native => "native".to_string(),
        VllmManagementMode::Mock => "mock".to_string(),
    }
}

#[get("/api/v1/vllm/capabilities")]
pub async fn get_capabilities(
    OptionalClaims(claims): OptionalClaims,
    Inject(config): Inject<JwtConfig>,
) -> Json<Capabilities> {
    Json(Capabilities {
        models_dashboard: models_dashboard_enabled(),
        vllm_management: vllm_management_enabled(),
        can_launch: can(claims.as_ref(), &config, "launch"),
        can_stop: can(claims.as_ref(), &config, "stop"),
        can_delete_model: can(claims.as_ref(), &config, "delete-model"),
        namespace: get_vllm_namespace(),
    })
}
