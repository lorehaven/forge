use std::sync::Arc;

use crate::llm::{Content, Llm, LlmRequest, Part};

pub async fn call_model(
    model: Arc<dyn Llm>,
    prompt: String,
    temperature: f32,
    max_tokens: usize,
) -> anyhow::Result<String> {
    let request = LlmRequest {
        model: model.name().to_string(),
        contents: vec![Content::new("user").with_text(prompt)],
        temperature,
        max_tokens,
    };

    let resp = model.generate_content(request).await?;

    let mut full = String::new();

    if let Some(content) = resp.content {
        for part in content.parts {
            let Part::Text(text) = part;
            full.push_str(&text);
        }
    }

    Ok(full)
}
