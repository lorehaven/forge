//! `vllm_pod_name` - the Pod/Service name (and matching label value) switchboard derives
//! from a model name. Kubernetes Service names and label values are capped at 63 characters,
//! so a long model name (especially with an org/repo prefix) needs to be shortened rather
//! than sent to the API and rejected.

use quench_serde::Value;
use switchboard_service::routers::vllm::kubernetes::{
    hf_offline_env, is_restorable, pod_status, restored_pod, vllm_pod_name,
};

#[quench_test::test]
fn short_model_name_is_used_as_is() {
    assert_eq!(
        vllm_pod_name("Qwen/Qwen3-VL-8B-Instruct-FP8", 8000),
        "vllm-qwen-qwen3-vl-8b-instruct-fp8-8000"
    );
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
fn same_model_and_port_is_deterministic_across_calls() {
    let model = "dark-side-of-the-code/Qwen3-Coder-30B-A3B-Instruct-AWQ";
    assert_eq!(vllm_pod_name(model, 8000), vllm_pod_name(model, 8000));
}

#[quench_test::test]
fn different_ports_on_a_long_name_still_fit_and_differ() {
    let model = "dark-side-of-the-code/Qwen3-Coder-30B-A3B-Instruct-AWQ";
    let a = vllm_pod_name(model, 8000);
    let b = vllm_pod_name(model, 8001);
    assert_ne!(a, b);
    assert!(a.len() <= 63 && b.len() <= 63);
    assert!(a.ends_with("-8000") && b.ends_with("-8001"));
}

#[quench_test::test]
fn pods_run_hugging_face_offline_so_gated_local_models_do_not_hit_the_hub() {
    let env = hf_offline_env();
    assert_eq!(env["name"], "HF_HUB_OFFLINE");
    assert_eq!(env["value"], "1");
}

fn pod(status: &Value) -> Value {
    let manifest = quench_serde::json!({
        "metadata": {
            "name": "vllm-m-8000",
            "labels": { "app": "vllm", "vllm-port": "8000" },
            "annotations": { "vllm-model": "org/m", "vllm-restarts": "2" }
        },
        "spec": {
            "nodeName": "node-1",
            "containers": [{
                "name": "vllm",
                "volumeMounts": [
                    { "name": "hf-cache", "mountPath": "/c" },
                    { "name": "kube-api-access-abcde", "mountPath": "/var/run/secrets" }
                ]
            }],
            "volumes": [
                { "name": "hf-cache", "hostPath": { "path": "/c" } },
                { "name": "kube-api-access-abcde", "projected": { "sources": [] } }
            ]
        },
        "status": status
    });
    manifest
}

#[quench_test::test]
fn reboot_leftovers_are_restorable_but_crashes_are_not() {
    let completed = pod(&quench_serde::json!({ "phase": "Succeeded" }));
    assert_eq!(pod_status(&completed), "completed");
    let shutdown = pod(&quench_serde::json!({ "phase": "Failed", "reason": "Shutdown" }));
    assert_eq!(pod_status(&shutdown), "interrupted");
    let crashed = pod(&quench_serde::json!({ "phase": "Failed", "reason": "Error" }));
    assert_eq!(pod_status(&crashed), "Failed");

    assert!(is_restorable("completed") && is_restorable("interrupted"));
    assert!(!is_restorable("Failed") && !is_restorable("running") && !is_restorable("starting"));
}

#[quench_test::test]
fn restored_pod_keeps_spec_but_drops_scheduler_and_token_state() {
    let old = pod(&quench_serde::json!({ "phase": "Succeeded" }));
    let new = restored_pod(&old, 3).expect("restorable");

    assert_eq!(new["metadata"]["name"], "vllm-m-8000");
    let annotations = &new["metadata"]["annotations"];
    assert_eq!(annotations["vllm-model"], "org/m");
    assert_eq!(annotations["vllm-restarts"], "3");
    assert_eq!(new["metadata"]["labels"]["vllm-port"], "8000");

    let spec = &new["spec"];
    assert!(spec.get("nodeName").is_none());
    let names = |items: &Value| -> Vec<String> {
        items
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(names(&spec["volumes"]), vec!["hf-cache".to_string()]);
    assert_eq!(
        names(&spec["containers"][0]["volumeMounts"]),
        vec!["hf-cache".to_string()]
    );
}
