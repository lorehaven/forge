//! `task_launch_args`/`task_from_args` - the vLLM `--task` flag translation
//! and its inverse, used when reconstructing instance state from a live
//! process's CLI args.

use crate::env_support::env_lock;
use switchboard_service::routers::vllm::types::{
    cpu_image, device_from_args, device_launch_args, is_cpu_device, task_from_args,
    task_launch_args,
};

fn parts(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

#[test]
fn task_launch_args_maps_embed_and_embedding_to_pooling_convert_embed() {
    assert_eq!(
        task_launch_args("embed"),
        vec!["--runner", "pooling", "--convert", "embed"]
    );
    assert_eq!(
        task_launch_args("embedding"),
        vec!["--runner", "pooling", "--convert", "embed"]
    );
}

#[test]
fn task_launch_args_maps_classify_to_pooling_convert_classify() {
    assert_eq!(
        task_launch_args("classify"),
        vec!["--runner", "pooling", "--convert", "classify"]
    );
}

#[test]
fn task_launch_args_maps_generate_to_runner_generate() {
    assert_eq!(task_launch_args("generate"), vec!["--runner", "generate"]);
}

#[test]
fn task_launch_args_passes_unknown_values_through_as_runner() {
    assert_eq!(task_launch_args("auto"), vec!["--runner", "auto"]);
    assert_eq!(task_launch_args("draft"), vec!["--runner", "draft"]);
}

#[test]
fn task_from_args_prefers_convert_embed_over_runner() {
    let args = parts(&["--runner", "generate", "--convert", "embed"]);
    assert_eq!(task_from_args(&args), Some("embed".to_string()));
}

#[test]
fn task_from_args_prefers_convert_classify_over_runner() {
    let args = parts(&["--convert", "classify"]);
    assert_eq!(task_from_args(&args), Some("classify".to_string()));
}

#[test]
fn task_from_args_falls_back_to_runner_pooling_as_embed() {
    let args = parts(&["--runner", "pooling"]);
    assert_eq!(task_from_args(&args), Some("embed".to_string()));
}

#[test]
fn task_from_args_falls_back_to_runner_generate() {
    let args = parts(&["--runner", "generate"]);
    assert_eq!(task_from_args(&args), Some("generate".to_string()));
}

#[test]
fn task_from_args_falls_back_to_the_legacy_task_flag() {
    let args = parts(&["--task", "embed"]);
    assert_eq!(task_from_args(&args), Some("embed".to_string()));
}

#[test]
fn task_from_args_is_none_when_nothing_matches() {
    let args = parts(&["--dtype", "float16"]);
    assert_eq!(task_from_args(&args), None);
}

#[test]
fn task_from_args_ignores_a_flag_with_no_following_value() {
    let args = parts(&["--runner"]);
    assert_eq!(task_from_args(&args), None);
}

#[test]
fn is_cpu_device_matches_cpu_case_insensitively_and_nothing_else() {
    assert!(is_cpu_device(Some("cpu")));
    assert!(is_cpu_device(Some("CPU")));
    assert!(is_cpu_device(Some(" cpu ")));
    assert!(!is_cpu_device(Some("gpu")));
    assert!(!is_cpu_device(Some("cuda")));
    assert!(!is_cpu_device(None));
}

#[test]
fn device_launch_args_is_empty_for_the_runtime_selected_devices() {
    // cpu included: CPU is chosen by the runtime/image, and current vLLM
    // rejects `--device cpu`.
    for value in ["", "gpu", "GPU", "auto", "default", "cpu", "CPU", " cpu "] {
        assert!(
            device_launch_args(value).is_empty(),
            "expected no flags for {value:?}"
        );
    }
}

#[test]
fn device_launch_args_passes_explicit_non_cpu_accelerators_through() {
    assert_eq!(device_launch_args("cuda"), vec!["--device", "cuda"]);
    assert_eq!(device_launch_args("neuron"), vec!["--device", "neuron"]);
}

#[test]
fn device_from_args_recovers_the_device_flag_value() {
    let args = parts(&["serve", "m", "--device", "cpu", "--port", "8000"]);
    assert_eq!(device_from_args(&args), Some("cpu".to_string()));
    assert_eq!(device_from_args(&parts(&["--port", "8000"])), None);
    assert_eq!(device_from_args(&parts(&["--device"])), None);
}

#[tokio::test]
async fn cpu_image_defaults_to_upstream_and_honours_the_env_override() {
    let _guard = env_lock().lock().await;
    unsafe { std::env::remove_var("VLLM_CPU_IMAGE") };
    assert_eq!(cpu_image(), "vllm/vllm-openai-cpu:latest-x86_64");
    unsafe {
        std::env::set_var(
            "VLLM_CPU_IMAGE",
            "reg.local/forge/vllm/vllm-openai-cpu:v0.25.1",
        )
    };
    assert_eq!(cpu_image(), "reg.local/forge/vllm/vllm-openai-cpu:v0.25.1");
    unsafe { std::env::remove_var("VLLM_CPU_IMAGE") };
}
