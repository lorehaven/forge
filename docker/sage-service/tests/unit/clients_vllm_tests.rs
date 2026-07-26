//! Unit tests for `clients/vllm.rs`.

use sage_service::clients::vllm::*;

#[test]
fn text_only_message_serializes_content_as_string() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "hello".to_string(),
        tool_calls: None,
        images: None,
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"role": "user", "content": "hello"})
    );
}

#[test]
fn message_with_images_serializes_content_parts() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "what is this?".to_string(),
        tool_calls: None,
        images: Some(vec!["data:image/png;base64,AAAA".to_string()]),
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "role": "user",
            "content": [
                {"type": "text", "text": "what is this?"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
            ]
        })
    );
}

#[test]
fn message_with_empty_images_serializes_as_text() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "hello".to_string(),
        tool_calls: None,
        images: Some(vec![]),
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["content"], serde_json::json!("hello"));
}

/// The exact streamed chunks from a Qwen tool call (name in one chunk,
/// arguments in the next) must now deserialize; previously the strict
/// `ToolCall` shape rejected these partial deltas and the calls were lost.
#[test]
fn streamed_tool_call_chunks_deserialize() {
    let name_chunk = r#"{"choices":[{"index":0,"delta":{"content":null,"tool_calls":[{"id":"chatcmpl-tool-1","type":"function","index":0,"function":{"name":"file_list"}}]},"finish_reason":null}]}"#;
    let args_chunk = r#"{"choices":[{"index":0,"delta":{"content":null,"tool_calls":[{"index":0,"function":{"arguments":"{}"}}]},"finish_reason":null}]}"#;

    let mut accum: std::collections::BTreeMap<usize, (String, String)> =
        std::collections::BTreeMap::new();

    for data in [name_chunk, args_chunk] {
        let chunk: ChatCompletionChunk =
            serde_json::from_str(data).expect("chunk should deserialize");
        let choice = &chunk.choices[0];
        assert!(choice.delta.content.is_none());
        for tc in choice.delta.tool_calls.iter().flatten() {
            let entry = accum.entry(tc.index).or_default();
            if let Some(func) = &tc.function {
                if let Some(n) = &func.name {
                    entry.0 = n.clone();
                }
                if let Some(a) = &func.arguments {
                    entry.1.push_str(a);
                }
            }
        }
    }

    assert_eq!(
        accum.get(&0),
        Some(&("file_list".to_string(), "{}".to_string()))
    );
}

/// Accumulated tool calls flush to a `<tool_call>` line that the text parser
/// turns back into a concrete tool call.
#[test]
fn flush_produces_parseable_tool_call() {
    let mut accum = std::collections::BTreeMap::new();
    accum.insert(0usize, ("file_list".to_string(), String::new()));
    accum.insert(
        1usize,
        (
            "file_search".to_string(),
            r#"{"query": "empatia"}"#.to_string(),
        ),
    );

    let synth = flush_streamed_tool_calls(&mut accum);
    assert_eq!(synth.len(), 2);
    assert!(accum.is_empty(), "flush should drain the accumulator");

    let combined = synth.join("\n");
    let calls = sage_service::tools::parser::parse_tool_calls(&combined);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "file_list");
    assert!(calls[0].arguments.as_object().unwrap().is_empty());
    assert_eq!(calls[1].name, "file_search");
    assert_eq!(calls[1].arguments["query"], "empatia");
}

#[test]
fn flush_skips_nameless_entries() {
    let mut accum = std::collections::BTreeMap::new();
    accum.insert(0usize, (String::new(), "{}".to_string()));
    assert!(flush_streamed_tool_calls(&mut accum).is_empty());
}
