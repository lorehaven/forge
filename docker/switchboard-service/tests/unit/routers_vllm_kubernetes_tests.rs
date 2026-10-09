//! `vllm_pod_name` - the Pod/Service name (and matching label value) switchboard derives
//! from a model name. Kubernetes Service names and label values are capped at 63 characters,
//! so a long model name (especially with an org/repo prefix) needs to be shortened rather
//! than sent to the API and rejected.

use switchboard_service::routers::vllm::kubernetes::{hf_offline_env, vllm_pod_name};

#[test]
fn short_model_name_is_used_as_is() {
    assert_eq!(
        vllm_pod_name("Qwen/Qwen3-VL-8B-Instruct-FP8", 8000),
        "vllm-qwen-qwen3-vl-8b-instruct-fp8-8000"
    );
}

#[test]
fn long_model_name_is_truncated_to_fit_the_63_char_label_limit() {
    // The real case this was found from: org prefix + repo name pushes the
    // naive "vllm-<model>-<port>" name to 64 characters.
    let name = vllm_pod_name(
        "dark-side-of-the-code/Qwen3-Coder-30B-A3B-Instruct-AWQ",
        8000,
    );
    assert!(name.len() <= 63, "{name} is {} chars", name.len());
    assert!(name.starts_with("vllm-dark-side-of-the-code"));
    assert!(name.ends_with("-8000"));
}

#[test]
fn truncated_name_is_a_valid_rfc_1035_label() {
    let name = vllm_pod_name(
        "dark-side-of-the-code/Qwen3-Coder-30B-A3B-Instruct-AWQ",
        8000,
    );
    assert!(
        name.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "{name} has characters outside [a-z0-9-]"
    );
    assert!(!name.starts_with('-') && !name.ends_with('-'), "{name}");
}

#[test]
fn distinct_long_names_sharing_a_truncated_prefix_do_not_collide() {
    let a = vllm_pod_name(
        "some-org/a-very-long-model-name-that-is-nearly-identical-one",
        8000,
    );
    let b = vllm_pod_name(
        "some-org/a-very-long-model-name-that-is-nearly-identical-two",
        8000,
    );
    assert_ne!(a, b);
    assert!(a.len() <= 63 && b.len() <= 63);
}

#[test]
fn same_model_and_port_is_deterministic_across_calls() {
    let model = "dark-side-of-the-code/Qwen3-Coder-30B-A3B-Instruct-AWQ";
    assert_eq!(vllm_pod_name(model, 8000), vllm_pod_name(model, 8000));
}

#[test]
fn different_ports_on_a_long_name_still_fit_and_differ() {
    let model = "dark-side-of-the-code/Qwen3-Coder-30B-A3B-Instruct-AWQ";
    let a = vllm_pod_name(model, 8000);
    let b = vllm_pod_name(model, 8001);
    assert_ne!(a, b);
    assert!(a.len() <= 63 && b.len() <= 63);
    assert!(a.ends_with("-8000") && b.ends_with("-8001"));
}

#[test]
fn pods_run_hugging_face_offline_so_gated_local_models_do_not_hit_the_hub() {
    let env = hf_offline_env();
    assert_eq!(env["name"], "HF_HUB_OFFLINE");
    assert_eq!(env["value"], "1");
}
