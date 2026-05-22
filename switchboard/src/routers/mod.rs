use crate::routers::gpu::GpuApiDoc;
use crate::routers::models::ModelsApiDoc;
use quench_srv::prelude::routers::BaseOpenApiDoc;
use std::sync::LazyLock;
use utoipa::OpenApi;

pub mod gpu;
pub mod models;
pub mod ui;

struct FeatureFlags {
    models_dashboard: bool,
    vllm_management: bool,
}

static FEATURE_FLAGS: LazyLock<FeatureFlags> = LazyLock::new(|| FeatureFlags {
    models_dashboard: feature_enabled("FEATURE_MODELS_DASHBOARD_ENABLED", false),
    vllm_management: feature_enabled("FEATURE_VLLM_MANAGEMENT_ENABLED", false),
});

fn feature_enabled(name: &str, default: bool) -> bool {
    match envmnt::get_or(name, if default { "true" } else { "false" })
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

pub fn models_dashboard_enabled() -> bool {
    FEATURE_FLAGS.models_dashboard
}

pub fn vllm_management_enabled() -> bool {
    FEATURE_FLAGS.vllm_management
}

pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = BaseOpenApiDoc::openapi();
    doc.merge(GpuApiDoc::openapi());
    doc.merge(ModelsApiDoc::openapi());
    doc
}
