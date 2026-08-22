use welder::model::switchboard_client::{LaunchInstanceRequest, SwitchboardClient, VllmInstance};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn instance(status: &str, task: Option<&str>) -> VllmInstance {
    VllmInstance {
        model: "llama".to_string(),
        host: "10.0.0.1".to_string(),
        port: 8000,
        status: status.to_string(),
        task: task.map(str::to_string),
    }
}

#[test]
fn is_chat_capable_excludes_embed_and_embedding_tasks() {
    assert!(instance("running", None).is_chat_capable());
    assert!(!instance("running", Some("embed")).is_chat_capable());
    assert!(!instance("running", Some("embedding")).is_chat_capable());
    assert!(instance("running", Some("generate")).is_chat_capable());
}

#[test]
fn is_running_and_is_failed_check_the_status_string() {
    assert!(instance("running", None).is_running());
    assert!(!instance("starting", None).is_running());
    assert!(instance("failed", None).is_failed());
    assert!(!instance("running", None).is_failed());
}

#[test]
fn address_joins_host_and_port() {
    assert_eq!(instance("running", None).address(), "10.0.0.1:8000");
}

#[tokio::test]
async fn list_instances_parses_the_response_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![instance("running", None)]))
        .mount(&server)
        .await;

    let client = SwitchboardClient::for_tests(&server.uri());
    let instances = client.list_instances().await.expect("list instances");
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].model, "llama");
}

#[tokio::test]
async fn list_instances_reports_a_non_success_response_as_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let client = SwitchboardClient::for_tests(&server.uri());
    let error = client.list_instances().await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("failed to list switchboard vLLM instances")
    );
}

#[tokio::test]
async fn launch_instance_parses_the_created_instance() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/vllm/instances"))
        .respond_with(ResponseTemplate::new(200).set_body_json(instance("starting", None)))
        .mount(&server)
        .await;

    let client = SwitchboardClient::for_tests(&server.uri());
    let request = LaunchInstanceRequest {
        model: "llama".to_string(),
        host: "0.0.0.0".to_string(),
        port: 8000,
        namespace: None,
        quantization: None,
        dtype: None,
        limit_mm_per_prompt: None,
        max_model_len: None,
        gpu_memory_utilization: None,
        enable_prefix_caching: false,
        enable_tool_calling: false,
        task: None,
    };

    let launched = client.launch_instance(request).await.expect("launch");
    assert_eq!(launched.status, "starting");
}
