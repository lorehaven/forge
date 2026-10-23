//! `vllm_pod_name` - the Pod/Service name (and matching label value) switchboard derives
//! from a model name. Kubernetes Service names and label values are capped at 63 characters,
//! so a long model name (especially with an org/repo prefix) needs to be shortened rather
//! than sent to the API and rejected.

use k8s_openapi::api::core::v1::Pod;
use switchboard_service::routers::vllm::kubernetes::{
    hf_offline_env, is_restorable, pod_status, restored_pod, vllm_pod_name,
};

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

fn pod(status: &quench_serde::Value) -> Pod {
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
    // kube's `Pod` is a serde type: the JSON text is the bridge from quench's `Value`.
    serde_json::from_str(&manifest.to_string()).expect("valid pod")
}

#[test]
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

#[test]
fn restored_pod_keeps_spec_but_drops_scheduler_and_token_state() {
    let old = pod(&quench_serde::json!({ "phase": "Succeeded" }));
    let new = restored_pod(&old, 3).expect("restorable");

    assert_eq!(new.metadata.name.as_deref(), Some("vllm-m-8000"));
    let annotations = new.metadata.annotations.unwrap();
    assert_eq!(annotations["vllm-model"], "org/m");
    assert_eq!(annotations["vllm-restarts"], "3");

    let spec = new.spec.unwrap();
    assert!(spec.node_name.is_none());
    let volumes: Vec<_> = spec.volumes.unwrap().into_iter().map(|v| v.name).collect();
    assert_eq!(volumes, vec!["hf-cache".to_string()]);
    let mounts: Vec<_> = spec.containers[0]
        .volume_mounts
        .clone()
        .unwrap()
        .into_iter()
        .map(|m| m.name)
        .collect();
    assert_eq!(mounts, vec!["hf-cache".to_string()]);
}
