//! Unit tests for `routers/ui/pages/initializing.rs`.

use chrono::Utc;
use sage_service::clients::switchboard::VllmInstance;
use sage_service::config::DefaultModel;
use sage_service::routers::ui::pages::initializing::*;

fn model(name: &str) -> DefaultModel {
    DefaultModel {
        name: name.to_string(),
        gpu_memory_utilization: None,
        max_model_len: None,
        quantization: None,
        dtype: None,
        limit_mm_per_prompt: None,
        enable_tool_calling: false,
        task: None,
        device: None,
    }
}

fn instance(model: &str, status: &str) -> VllmInstance {
    VllmInstance {
        id: format!("pid-{model}"),
        namespace: "native".to_string(),
        model: model.to_string(),
        host: "0.0.0.0".to_string(),
        port: 8000,
        quantization: None,
        max_model_len: None,
        gpu_memory_utilization: None,
        enable_prefix_caching: false,
        task: None,
        device: None,
        started_at: Utc::now(),
        status: status.to_string(),
    }
}

#[test]
fn healthiest_matching_instance_wins() {
    let m = model("qwen");
    let insts = vec![instance("qwen", "failed"), instance("qwen", "running")];
    assert_eq!(model_state(&m, &insts), ModelState::Running);
}

#[test]
fn missing_instance_is_pending() {
    let m = model("qwen");
    assert_eq!(model_state(&m, &[]), ModelState::Pending);
}

#[test]
fn starting_and_pending_map_to_starting() {
    let m = model("qwen");
    assert_eq!(
        model_state(&m, &[instance("qwen", "starting")]),
        ModelState::Starting
    );
    assert_eq!(
        model_state(&m, &[instance("qwen", "pending")]),
        ModelState::Starting
    );
}

#[test]
fn all_running_requires_every_default_model() {
    let defaults = vec![model("chat"), model("embed")];
    let insts = vec![instance("chat", "running"), instance("embed", "starting")];
    assert!(!all_models_running(&defaults, &insts));

    let ready = vec![instance("chat", "running"), instance("embed", "running")];
    assert!(all_models_running(&defaults, &ready));
}

#[test]
fn no_default_models_is_ready() {
    assert!(all_models_running(&[], &[]));
}
