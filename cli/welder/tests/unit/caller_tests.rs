use std::sync::{Arc, Mutex};
use welder::llm::{Content, Llm, LlmRequest, LlmResponse};
use welder::model::caller::call_model;

#[derive(Debug)]
struct StubLlm {
    name: &'static str,
    reply: &'static str,
    /// The last request this stub was called with, for assertions - a
    /// `Mutex` because `Llm::generate_content` only takes `&self`.
    last_request: Mutex<Option<LlmRequest>>,
}

#[async_trait::async_trait]
impl Llm for StubLlm {
    fn name(&self) -> &str {
        self.name
    }

    async fn generate_content(&self, request: LlmRequest) -> anyhow::Result<LlmResponse> {
        *self.last_request.lock().unwrap() = Some(request);
        Ok(LlmResponse {
            content: Some(Content::new("assistant").with_text(self.reply)),
        })
    }
}

#[derive(Debug)]
struct FailingLlm;

#[async_trait::async_trait]
impl Llm for FailingLlm {
    fn name(&self) -> &'static str {
        "failing"
    }

    async fn generate_content(&self, _request: LlmRequest) -> anyhow::Result<LlmResponse> {
        Err(anyhow::anyhow!("model unavailable"))
    }
}

#[tokio::test]
async fn call_model_returns_the_joined_text_and_forwards_the_prompt() {
    let stub = Arc::new(StubLlm {
        name: "stub",
        reply: "hello back",
        last_request: Mutex::new(None),
    });

    let result = call_model(stub.clone(), "hi there".to_string(), 0.5, 64)
        .await
        .expect("call succeeds");

    assert_eq!(result, "hello back");
    let sent = stub
        .last_request
        .lock()
        .unwrap()
        .clone()
        .expect("request recorded");
    assert_eq!(sent.model, "stub");
    assert!((sent.temperature - 0.5).abs() < f32::EPSILON);
    assert_eq!(sent.max_tokens, 64);
}

/// `StubLlm` always wraps a `Some(Content)`, so exercise the "no content at
/// all" branch of `call_model` with a model that returns `None`.
#[derive(Debug)]
struct NoContentLlm;

#[async_trait::async_trait]
impl Llm for NoContentLlm {
    fn name(&self) -> &'static str {
        "no-content"
    }

    async fn generate_content(&self, _request: LlmRequest) -> anyhow::Result<LlmResponse> {
        Ok(LlmResponse { content: None })
    }
}

#[tokio::test]
async fn call_model_is_empty_when_the_model_returns_no_content() {
    let result = call_model(Arc::new(NoContentLlm), "hi".to_string(), 0.0, 1)
        .await
        .expect("call succeeds");
    assert_eq!(result, "");
}

#[tokio::test]
async fn call_model_propagates_a_model_error() {
    let error = call_model(Arc::new(FailingLlm), "hi".to_string(), 0.0, 1)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("model unavailable"));
}
