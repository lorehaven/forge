use welder::llm::ollama::OllamaModel;
use welder::llm::{Content, Llm, LlmRequest, Part};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request(text: &str) -> LlmRequest {
    LlmRequest {
        model: "llama".to_string(),
        contents: vec![Content::new("user").with_text(text)],
        temperature: 0.4,
        max_tokens: 256,
    }
}

#[test]
fn name_returns_the_configured_model() {
    let model = OllamaModel::new("llama", "http://127.0.0.1:1").expect("build");
    assert_eq!(model.name(), "llama");
}

#[tokio::test]
async fn generate_content_parses_the_message_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "message": { "content": "hi there" }
        })))
        .mount(&server)
        .await;

    let model = OllamaModel::new("llama", server.uri()).expect("build");
    let response = model
        .generate_content(request("hello"))
        .await
        .expect("generate");
    let Part::Text(text) = &response.content.expect("content").parts[0];
    assert_eq!(text, "hi there");
}

#[tokio::test]
async fn generate_content_surfaces_a_non_success_response_as_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let model = OllamaModel::new("llama", server.uri()).expect("build");
    let error = model.generate_content(request("hello")).await.unwrap_err();
    assert!(error.to_string().contains("boom"));
}

#[tokio::test]
async fn generate_content_reports_an_unparseable_response_as_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;

    let model = OllamaModel::new("llama", server.uri()).expect("build");
    let error = model.generate_content(request("hello")).await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Failed to parse Ollama response")
    );
}
