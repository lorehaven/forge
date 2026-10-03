use gantry_service::domain::settings::{ClusterMode, RunnerMode, Settings};
use std::collections::HashMap;

fn settings(pairs: &[(&str, &str)]) -> Result<Settings, String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    Settings::from_lookup(|key| map.get(key).cloned())
}

#[test]
fn with_nothing_set_there_is_no_cluster_and_nothing_can_change() {
    let settings = settings(&[]).unwrap();
    assert_eq!(settings.cluster, ClusterMode::None);
    assert_eq!(settings.runner, RunnerMode::DryRun);
}

#[test]
fn a_pod_defaults_to_its_own_cluster_and_real_jobs() {
    let settings = settings(&[("KUBERNETES_SERVICE_HOST", "10.0.0.1")]).unwrap();
    assert_eq!(settings.cluster, ClusterMode::InCluster);
    assert_eq!(settings.runner, RunnerMode::Job);
}

#[test]
fn reading_a_developers_cluster_does_not_by_itself_let_anything_change_it() {
    let settings = settings(&[("GANTRY_CLUSTER", "kubeconfig")]).unwrap();
    assert_eq!(settings.cluster, ClusterMode::Kubeconfig);
    assert_eq!(settings.runner, RunnerMode::DryRun);
}

#[test]
fn changing_a_developers_cluster_has_to_be_said_twice() {
    let settings =
        settings(&[("GANTRY_CLUSTER", "kubeconfig"), ("GANTRY_RUNNER", "local")]).unwrap();
    assert_eq!(settings.runner, RunnerMode::Local);
}

#[test]
fn a_local_runner_is_refused_without_a_real_kubeconfig_cluster() {
    let error = settings(&[("GANTRY_RUNNER", "local")]).unwrap_err();
    assert!(error.contains("kubeconfig"), "{error}");
    assert!(settings(&[("GANTRY_CLUSTER", "none"), ("GANTRY_RUNNER", "local")]).is_err());
}

#[test]
fn a_job_runner_needs_a_cluster_to_put_the_job_in() {
    let error = settings(&[("GANTRY_RUNNER", "job")]).unwrap_err();
    assert!(error.contains("needs a cluster"), "{error}");
}

#[test]
fn unknown_modes_are_errors_not_silent_defaults() {
    assert!(settings(&[("GANTRY_CLUSTER", "prod")]).is_err());
    assert!(settings(&[("GANTRY_RUNNER", "yolo")]).is_err());
}

#[test]
fn the_allow_list_is_a_trimmed_comma_list() {
    let settings = settings(&[("GANTRY_NAMESPACES", " a, b ,,c")]).unwrap();
    assert_eq!(settings.allowed_namespaces, ["a", "b", "c"]);
}
