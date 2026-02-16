use std::collections::HashMap;
use std::sync::Arc;

use adk_core::{Content, Llm, LlmRequest, Part};
use futures::StreamExt;

pub async fn call_model(model: Arc<dyn Llm>, prompt: String) -> anyhow::Result<String> {
    let request = LlmRequest {
        model: "llama3.1:8b".to_string(),
        contents: vec![Content::new("user").with_text(prompt)],
        tools: HashMap::default(),
        config: None,
    };

    let stream = model.generate_content(request, false).await?;
    futures::pin_mut!(stream);

    let mut full = String::new();

    while let Some(resp) = stream.next().await {
        let resp = resp?;

        if let Some(content) = resp.content {
            for part in content.parts {
                if let Part::Text { text } = part {
                    full.push_str(&text);
                }
            }
        }
    }

    Ok(full)
}
