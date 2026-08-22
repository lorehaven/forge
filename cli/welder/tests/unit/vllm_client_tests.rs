use welder::llm::{Content, Llm, LlmRequest, Part};
use welder::model::vllm_client::{VllmConfig, VllmModel};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request(text: &str) -> LlmRequest {
    LlmRequest {
        model: "test-model".to_string(),
        contents: vec![Content::new("user").with_text(text)],
        temperature: 0.7,
        max_tokens: 128,
    }
}

#[test]
fn config_new_defaults_to_localhost_8000() {
    let config = VllmConfig::new("llama");
    assert_eq!(config.model, "llama");
    assert_eq!(config.host, "http://127.0.0.1:8000");
}

#[test]
fn name_returns_the_configured_model() {
    let model = VllmModel::new(VllmConfig::new("llama")).expect("build");
    assert_eq!(model.name(), "llama");
}

#[tokio::test]
async fn generate_content_parses_the_first_choice() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{ "message": { "content": "hi there" } }]
        })))
        .mount(&server)
        .await;

    let model = VllmModel::new(VllmConfig {
        model: "llama".to_string(),
        host: server.uri(),
    })
    .expect("build");

    let response = model
        .generate_content(request("hello"))
        .await
        .expect("generate");
    let Some(content) = response.content else {
        panic!("expected content");
    };
    let Part::Text(text) = &content.parts[0];
    assert_eq!(text, "hi there");
}

#[tokio::test]
async fn generate_content_defaults_when_choices_is_empty() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "choices": [] })),
        )
        .mount(&server)
        .await;

    let model = VllmModel::new(VllmConfig {
        model: "llama".to_string(),
        host: server.uri(),
    })
    .expect("build");

    let response = model
        .generate_content(request("hello"))
        .await
        .expect("generate");
    let Part::Text(text) = &response.content.expect("content").parts[0];
    assert_eq!(text, "No response");
}

#[tokio::test]
async fn generate_content_surfaces_a_non_success_response_as_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let model = VllmModel::new(VllmConfig {
        model: "llama".to_string(),
        host: server.uri(),
    })
    .expect("build");

    let error = model.generate_content(request("hello")).await.unwrap_err();
    assert!(error.to_string().contains("boom"));
}
