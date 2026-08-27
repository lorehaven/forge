//! `render_instances_grid` - the vLLM instance dashboard's HTML, and the
//! status/fit-class branching it does per instance.

use chrono::Utc;
use switchboard_service::routers::vllm::list::render_instances_grid;
use switchboard_service::routers::vllm::types::VllmInstance;

fn instance(status: &str) -> VllmInstance {
    VllmInstance {
        id: "inst-1".to_string(),
        namespace: "default".to_string(),
        model: "llama-3-8b".to_string(),
        host: "0.0.0.0".to_string(),
        port: 8000,
        quantization: None,
        dtype: None,
        limit_mm_per_prompt: None,
        max_model_len: None,
        gpu_memory_utilization: None,
        enable_prefix_caching: false,
        enable_tool_calling: false,
        task: None,
        device: None,
        started_at: Utc::now(),
        status: status.to_string(),
        log_path: None,
        last_error: None,
    }
}

#[test]
fn empty_instance_list_renders_the_empty_state() {
    let html = render_instances_grid(vec![], true);
    assert!(html.contains("vllm-instances-grid"));
    assert!(html.contains("ui_vllm_no_instances"));
}

#[test]
fn running_instance_gets_the_ok_fit_class_and_a_stop_button_when_allowed() {
    let html = render_instances_grid(vec![instance("running")], true);
    assert!(html.contains("status-running"));
    assert!(html.contains("fit-ok"));
    assert!(html.contains("card-delete"));
    assert!(html.contains("llama-3-8b"));
}

#[test]
fn failed_instance_never_gets_a_stop_button_even_when_allowed() {
    let html = render_instances_grid(vec![instance("failed")], true);
    assert!(html.contains("status-failed"));
    assert!(html.contains("fit-no"));
    assert!(!html.contains("card-delete"));
}

#[test]
fn terminating_instance_never_gets_a_stop_button_either() {
    let html = render_instances_grid(vec![instance("terminating")], true);
    assert!(html.contains("status-terminating"));
    assert!(html.contains("fit-warn"));
    assert!(!html.contains("card-delete"));
}

#[test]
fn starting_instance_gets_the_warn_fit_class() {
    let html = render_instances_grid(vec![instance("starting")], true);
    assert!(html.contains("status-starting"));
    assert!(html.contains("fit-warn"));
}

#[test]
fn stop_button_is_omitted_when_the_caller_cannot_stop() {
    let html = render_instances_grid(vec![instance("running")], false);
    assert!(!html.contains("card-delete"));
}

#[test]
fn dtype_and_limit_mm_per_prompt_appear_in_the_fit_line_when_present() {
    let mut model = instance("running");
    model.dtype = Some("float16".to_string());
    model.limit_mm_per_prompt = Some(r#"{"image": 4}"#.to_string());
    let html = render_instances_grid(vec![model], true);
    assert!(html.contains("float16"));
    assert!(html.contains("ui_vllm_form_dtype"));
    assert!(html.contains("ui_vllm_form_limit_mm"));
}

#[test]
fn diagnostics_block_is_hidden_when_there_is_no_error_or_log() {
    let html = render_instances_grid(vec![instance("running")], true);
    assert!(html.contains("instance-diagnostics"));
    assert!(html.contains("display: none;"));
}

#[test]
fn last_error_and_log_path_render_into_the_diagnostics_block() {
    let mut model = instance("failed");
    model.last_error = Some("CUDA out of memory".to_string());
    model.log_path = Some("/var/log/vllm/inst-1.log".to_string());
    let html = render_instances_grid(vec![model], true);
    assert!(html.contains("CUDA out of memory"));
    assert!(html.contains("/var/log/vllm/inst-1.log"));
}

#[test]
fn quantization_falls_back_to_auto_when_unset() {
    let html = render_instances_grid(vec![instance("running")], true);
    assert!(html.contains(">auto<") || html.contains("auto"));
}

#[test]
fn multiple_instances_all_render_as_separate_cards() {
    let mut a = instance("running");
    a.id = "a".to_string();
    a.model = "model-a".to_string();
    let mut b = instance("running");
    b.id = "b".to_string();
    b.model = "model-b".to_string();
    let html = render_instances_grid(vec![a, b], true);
    assert!(html.contains("model-a"));
    assert!(html.contains("model-b"));
}
