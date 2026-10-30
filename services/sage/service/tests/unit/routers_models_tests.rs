//! Unit tests for `routers/models.rs`.

use quench_core::time::Time;
use sage_service::clients::switchboard::VllmInstance;
use sage_service::config::DefaultModel;
use sage_service::routers::models::*;

fn model(name: &str) -> DefaultModel {
    DefaultModel {
        name: name.to_string(),
        gpu_memory_utilization: None,
        max_model_len: None,
        quantization: None,
        dtype: None,
        limit_mm_per_prompt: None,
        max_num_seqs: None,
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
        started_at: Time::now(),
        status: status.to_string(),
    }
}

#[quench_test::test]
fn healthiest_matching_instance_wins() {
    let m = model("qwen");
    let insts = vec![instance("qwen", "failed"), instance("qwen", "running")];
    assert_eq!(model_state(&m, &insts), ModelState::Running);
}

#[quench_test::test]
fn missing_instance_is_pending() {
    let m = model("qwen");
    assert_eq!(model_state(&m, &[]), ModelState::Pending);
}

#[quench_test::test]
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

#[quench_test::test]
fn all_running_requires_every_default_model() {
    let defaults = vec![model("chat"), model("embed")];
    let insts = vec![instance("chat", "running"), instance("embed", "starting")];
    assert!(!all_models_running(&defaults, &insts));

    let ready = vec![instance("chat", "running"), instance("embed", "running")];
    assert!(all_models_running(&defaults, &ready));
}

#[quench_test::test]
fn no_default_models_is_ready() {
    assert!(all_models_running(&[], &[]));
}

fn config_supporting(patterns: &[&str], defaults: &[&str]) -> sage_service::config::SageConfig {
    sage_service::config::SageConfig {
        system_prompt: String::new(),
        default_models: defaults.iter().map(|n| model(n)).collect(),
        supported_models: patterns.iter().map(ToString::to_string).collect(),
        default_search_provider: "none".to_string(),
        available_search_providers: vec![],
        capability_profile: sage_service::tools::capabilities::get_profile("web_assistant")
            .unwrap(),
        stop_models_on_shutdown: false,
    }
}

#[quench_test::test]
fn a_model_the_configuration_does_not_support_is_neither_offered_nor_waited_for() {
    let config = config_supporting(
        &["Qwen/*", "RedHatAI/*"],
        &["RedHatAI/Qwen3.5-4B", "losseheil/stepover"],
    );
    let instances = Ok(vec![
        instance("RedHatAI/Qwen3.5-4B", "running"),
        instance("losseheil/stepover", "running"),
    ]);
    let view = models_view(&config, &instances);
    let offered: Vec<_> = view.instances.iter().map(|i| i.model.as_str()).collect();
    assert_eq!(offered, ["RedHatAI/Qwen3.5-4B"]);
    let defaults: Vec<_> = view.defaults.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(defaults, ["RedHatAI/Qwen3.5-4B"]);
    assert!(
        view.ready,
        "the unsupported default must not hold the chat back"
    );
}
