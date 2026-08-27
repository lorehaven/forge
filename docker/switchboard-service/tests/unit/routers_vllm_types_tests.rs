//! `task_launch_args`/`task_from_args` - the vLLM `--task` flag translation
//! and its inverse, used when reconstructing instance state from a live
//! process's CLI args.

use switchboard_service::routers::vllm::types::{
    device_from_args, device_launch_args, is_cpu_device, task_from_args, task_launch_args,
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
fn device_launch_args_is_empty_for_gpu_defaults() {
    for value in ["", "gpu", "GPU", "auto", "default"] {
        assert!(
            device_launch_args(value).is_empty(),
            "expected no flags for {value:?}"
        );
    }
}

#[test]
fn device_launch_args_passes_cpu_and_other_devices_through() {
    assert_eq!(device_launch_args("cpu"), vec!["--device", "cpu"]);
    assert_eq!(device_launch_args("CPU"), vec!["--device", "cpu"]);
    assert_eq!(device_launch_args("cuda"), vec!["--device", "cuda"]);
}

#[test]
fn device_from_args_recovers_the_device_flag_value() {
    let args = parts(&["serve", "m", "--device", "cpu", "--port", "8000"]);
    assert_eq!(device_from_args(&args), Some("cpu".to_string()));
    assert_eq!(device_from_args(&parts(&["--port", "8000"])), None);
    assert_eq!(device_from_args(&parts(&["--device"])), None);
}
