use welder::model::switchboard_client::{self, VllmInstance};
use welder::model::{
    ModelInstanceConfig, ModelManager, SwitchboardManager, SwitchboardModelConfig, VllmConfig,
    extract_host_port,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config(model: &str, url: &str) -> ModelInstanceConfig {
    ModelInstanceConfig {
        model: model.to_string(),
        url: url.to_string(),
        model_path: None,
        dtype: "auto".to_string(),
        max_model_len: None,
        gpu_memory_utilization: 0.9,
        tensor_parallel_size: 1,
    }
}

#[test]
fn extract_host_port_splits_host_colon_port() {
    let (host, port) = extract_host_port("127.0.0.1:8000").expect("valid url");
    assert_eq!(host, "127.0.0.1");
    assert_eq!(port, 8000);
}

#[test]
fn extract_host_port_rejects_missing_port() {
    assert!(extract_host_port("127.0.0.1").is_err());
}

#[test]
fn extract_host_port_rejects_extra_colons() {
    assert!(extract_host_port("127.0.0.1:8000:extra").is_err());
}

#[test]
fn extract_host_port_rejects_a_non_numeric_port() {
    assert!(extract_host_port("127.0.0.1:not-a-port").is_err());
}

#[test]
fn model_manager_register_then_get_url_finds_the_matching_model() {
    let mut manager = ModelManager::default();
    manager.register(config("llama", "127.0.0.1:9001"));

    assert_eq!(manager.get_url("llama"), Some("127.0.0.1:9001".to_string()));
    assert_eq!(manager.get_url("unknown"), None);
}

#[test]
fn model_manager_is_port_available_reflects_a_real_bound_port() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();

    assert!(!ModelManager::is_port_available("127.0.0.1", port));

    drop(listener);
    assert!(ModelManager::is_port_available("127.0.0.1", port));
}

#[tokio::test]
async fn model_manager_initialize_is_a_no_op_with_nothing_registered() {
    let mut manager = ModelManager::default();
    manager.initialize().await.expect("nothing to initialize");
}

#[tokio::test]
async fn model_manager_initialize_skips_an_instance_whose_port_is_already_in_use() {
    // Holding a real listener on the configured port makes `initialize`
    // take its "already in use, reusing existing instance" branch instead
    // of trying to spawn a real `vllm serve` - this machine has a real
    // `vllm` on PATH, so that branch must never be reached in a test.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();

    let mut manager = ModelManager::default();
    manager.register(config("m", &format!("127.0.0.1:{port}")));

    manager
        .initialize()
        .await
        .expect("skips the already-bound port instead of erroring");

    drop(listener);
}

#[test]
fn vllm_config_default_is_five_minutes() {
    assert_eq!(VllmConfig::default().timeout_seconds, 300);
}

fn switchboard_config(model: &str) -> SwitchboardModelConfig {
    SwitchboardModelConfig {
        model: model.to_string(),
        quantization: None,
        dtype: None,
        max_model_len: None,
        gpu_memory_utilization: None,
        limit_mm_per_prompt: None,
        task: None,
    }
}

fn switchboard_client_for(server: &MockServer) -> switchboard_client::SwitchboardClient {
    switchboard_client::SwitchboardClient::for_tests(&server.uri())
}

#[test]
fn switchboard_manager_register_then_get_url_is_none_until_resolved() {
    let server_url = "http://127.0.0.1:1".to_string();
    let client = switchboard_client::SwitchboardClient::for_tests(&server_url);
    let mut manager = SwitchboardManager::new(client, 1);
    manager.register(switchboard_config("llama"));

    assert_eq!(manager.get_url("llama"), None);
}

#[tokio::test]
async fn switchboard_manager_initialize_is_a_no_op_with_nothing_registered() {
    let server_url = "http://127.0.0.1:1".to_string();
    let client = switchboard_client::SwitchboardClient::for_tests(&server_url);
    let mut manager = SwitchboardManager::new(client, 1);
    manager.initialize().await.expect("nothing to initialize");
}

#[tokio::test]
async fn switchboard_manager_reuses_an_already_running_instance() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![VllmInstance {
            model: "llama".to_string(),
            host: "10.0.0.5".to_string(),
            port: 8001,
            status: "running".to_string(),
            task: None,
        }]))
        .mount(&server)
        .await;

    let mut manager = SwitchboardManager::new(switchboard_client_for(&server), 1);
    manager.register(switchboard_config("llama"));
    manager
        .initialize()
        .await
        .expect("reuse the running instance");

    assert_eq!(manager.get_url("llama"), Some("10.0.0.5:8001".to_string()));
}

#[tokio::test]
async fn switchboard_manager_launches_and_waits_for_a_missing_instance() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(Vec::<VllmInstance>::new()))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(VllmInstance {
            model: "llama".to_string(),
            host: "10.0.0.6".to_string(),
            port: 8002,
            status: "starting".to_string(),
            task: None,
        }))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![VllmInstance {
            model: "llama".to_string(),
            host: "10.0.0.6".to_string(),
            port: 8002,
            status: "running".to_string(),
            task: None,
        }]))
        .mount(&server)
        .await;

    let mut manager = SwitchboardManager::new(switchboard_client_for(&server), 5);
    manager.register(switchboard_config("llama"));
    manager.initialize().await.expect("launch and become ready");

    assert_eq!(manager.get_url("llama"), Some("10.0.0.6:8002".to_string()));
}

#[tokio::test]
async fn switchboard_manager_wait_for_running_surfaces_a_failed_instance() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![VllmInstance {
            model: "llama".to_string(),
            host: "10.0.0.7".to_string(),
            port: 8003,
            status: "failed".to_string(),
            task: None,
        }]))
        .mount(&server)
        .await;

    let manager = SwitchboardManager::new(switchboard_client_for(&server), 1);
    let error = manager.wait_for_running("llama").await.unwrap_err();
    assert!(error.to_string().contains("failed to start"));
}

#[tokio::test]
async fn switchboard_manager_wait_for_running_gives_up_after_the_timeout() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(Vec::<VllmInstance>::new()))
        .mount(&server)
        .await;

    let manager = SwitchboardManager::new(switchboard_client_for(&server), 0);
    let error = manager.wait_for_running("llama").await.unwrap_err();
    assert!(error.to_string().contains("did not become ready"));
}

#[tokio::test]
async fn switchboard_manager_find_running_ignores_embedding_only_instances() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![VllmInstance {
            model: "embed-model".to_string(),
            host: "10.0.0.8".to_string(),
            port: 8004,
            status: "running".to_string(),
            task: Some("embed".to_string()),
        }]))
        .mount(&server)
        .await;

    let manager = SwitchboardManager::new(switchboard_client_for(&server), 1);
    assert_eq!(manager.find_running("embed-model").await.unwrap(), None);
}
