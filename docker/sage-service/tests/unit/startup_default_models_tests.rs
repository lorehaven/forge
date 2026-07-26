//! Unit tests for `startup/default_models.rs`.

use sage_service::clients::switchboard::VllmInstance;
use sage_service::config::{DefaultModel, SageConfig};
use sage_service::startup::default_models::*;
use sage_service::tools;
use std::collections::HashMap;

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
        started_at: chrono::Utc::now(),
        status: status.to_string(),
    }
}

fn test_config(models: &[&str]) -> SageConfig {
    SageConfig {
        system_prompt: String::new(),
        default_models: models.iter().map(|n| model(n)).collect(),
        supported_models: vec!["*".to_string()],
        default_search_provider: "none".to_string(),
        available_search_providers: vec![],
        capability_profile: tools::capabilities::get_profile("web_assistant").unwrap(),
        stop_models_on_shutdown: false,
    }
}

#[test]
fn launches_the_first_missing_model() {
    let config = test_config(&["chat", "embed"]);
    let next = next_model_to_launch(&config, &[], &HashMap::new()).unwrap();
    assert_eq!(next.name, "chat");
}

#[test]
fn waits_while_another_model_is_starting() {
    let config = test_config(&["chat", "embed"]);
    let instances = vec![instance("chat", "starting")];
    assert!(next_model_to_launch(&config, &instances, &HashMap::new()).is_none());

    let instances = vec![instance("chat", "pending")];
    assert!(next_model_to_launch(&config, &instances, &HashMap::new()).is_none());
}

#[test]
fn moves_on_once_the_previous_model_is_running() {
    let config = test_config(&["chat", "embed"]);
    let instances = vec![instance("chat", "running")];
    let next = next_model_to_launch(&config, &instances, &HashMap::new()).unwrap();
    assert_eq!(next.name, "embed");
}

#[test]
fn relaunches_a_failed_model_before_the_rest() {
    let config = test_config(&["chat", "embed"]);
    let instances = vec![instance("chat", "failed")];
    let next = next_model_to_launch(&config, &instances, &HashMap::new()).unwrap();
    assert_eq!(next.name, "chat");
}

#[test]
fn skips_a_model_that_exhausted_its_attempts() {
    let config = test_config(&["chat", "embed"]);
    let instances = vec![instance("chat", "failed")];
    let attempts = HashMap::from([("chat".to_string(), MAX_LAUNCH_ATTEMPTS)]);
    let next = next_model_to_launch(&config, &instances, &attempts).unwrap();
    assert_eq!(next.name, "embed");
}

#[test]
fn skips_unsupported_models() {
    let mut config = test_config(&["chat", "embed"]);
    config.supported_models = vec!["embed".to_string()];
    let next = next_model_to_launch(&config, &[], &HashMap::new()).unwrap();
    assert_eq!(next.name, "embed");
}

#[test]
fn nothing_to_launch_when_all_models_are_running() {
    let config = test_config(&["chat", "embed"]);
    let instances = vec![instance("chat", "running"), instance("embed", "running")];
    assert!(next_model_to_launch(&config, &instances, &HashMap::new()).is_none());
}
