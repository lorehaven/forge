pub mod gpu;
pub mod models;
pub mod ui;
pub mod vllm;

struct FeatureFlags {
    models_dashboard: bool,
    vllm_management: bool,
}

static FEATURE_FLAGS: std::sync::LazyLock<FeatureFlags> =
    std::sync::LazyLock::new(|| FeatureFlags {
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
