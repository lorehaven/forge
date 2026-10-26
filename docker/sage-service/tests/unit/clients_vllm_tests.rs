//! Unit tests for `clients/vllm.rs`.

use sage_service::clients::vllm::*;

#[quench_test::test]
fn text_only_message_serializes_content_as_string() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "hello".to_string(),
        tool_calls: None,
        images: None,
    };
    let json = quench_serde::to_value(&msg);
    assert_eq!(
        json,
        quench_serde::json!({"role": "user", "content": "hello"})
    );
}

#[quench_test::test]
fn message_with_images_serializes_content_parts() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "what is this?".to_string(),
        tool_calls: None,
        images: Some(vec!["data:image/png;base64,AAAA".to_string()]),
    };
    let json = quench_serde::to_value(&msg);
    assert_eq!(
        json,
        quench_serde::json!({
            "role": "user",
            "content": [
                {"type": "text", "text": "what is this?"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
            ]
        })
    );
}

#[quench_test::test]
fn message_with_empty_images_serializes_as_text() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "hello".to_string(),
        tool_calls: None,
        images: Some(vec![]),
    };
    let json = quench_serde::to_value(&msg);
    assert_eq!(json["content"], quench_serde::json!("hello"));
}

/// The exact streamed chunks from a Qwen tool call (name in one chunk,
/// arguments in the next) must now deserialize; previously the strict
/// `ToolCall` shape rejected these partial deltas and the calls were lost.
#[quench_test::test]
fn streamed_tool_call_chunks_deserialize() {
    let name_chunk = r#"{"choices":[{"index":0,"delta":{"content":null,"tool_calls":[{"id":"chatcmpl-tool-1","type":"function","index":0,"function":{"name":"file_list"}}]},"finish_reason":null}]}"#;
    let args_chunk = r#"{"choices":[{"index":0,"delta":{"content":null,"tool_calls":[{"index":0,"function":{"arguments":"{}"}}]},"finish_reason":null}]}"#;

    let mut accum: std::collections::BTreeMap<usize, (String, String)> =
        std::collections::BTreeMap::new();

    for data in [name_chunk, args_chunk] {
        let chunk: ChatCompletionChunk =
            quench_serde::json::from_str(data).expect("chunk should deserialize");
        let choice = &chunk.choices[0];
        assert!(choice.delta.content.is_none());
        for tc in choice.delta.tool_calls.iter().flatten() {
            let entry = accum.entry(tc.index).or_default();
            if let Some(func) = &tc.function {
                if let Some(n) = &func.name {
                    entry.0.clone_from(n);
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

/// Accumulated tool calls flush to a `<tool_call>` line the text parser turns back into a concrete tool call.
#[quench_test::test]
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

#[quench_test::test]
fn flush_skips_nameless_entries() {
    let mut accum = std::collections::BTreeMap::new();
    accum.insert(0usize, (String::new(), "{}".to_string()));
    assert!(flush_streamed_tool_calls(&mut accum).is_empty());
}

// ---- the HTTP calls, against a mock vLLM -----------------------------------------------------

use quench_core::stream::StreamExt;
use quench_test::mock::matchers::{method, path};
use quench_test::mock::{Mock, MockServer, ResponseTemplate};

const fn port_of(server: &MockServer) -> u16 {
    server.address().port()
}

async fn mount(server: &MockServer, route: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(route))
        .respond_with(response)
        .mount(server)
        .await;
}

fn json_response(status: u16, body: &quench_serde::Value) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_raw(quench_serde::json::to_string(body), "application/json")
}

fn sse_response(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_string(), "text/event-stream")
}

fn user(text: &str) -> Vec<ChatMessage> {
    vec![ChatMessage {
        role: "user".to_string(),
        content: text.to_string(),
        tool_calls: None,
        images: None,
    }]
}

async fn collect(
    stream: std::pin::Pin<
        Box<dyn quench_core::stream::Stream<Item = quench_core::Result<String>> + Send + Sync>,
    >,
) -> Vec<String> {
    let mut stream = stream;
    let mut out = Vec::new();
    while let Some(item) = stream.next().await {
        out.push(item.expect("a chunk"));
    }
    out
}

#[quench_test::test]
async fn chat_json_returns_the_message_content_and_sends_the_schema() {
    let server = MockServer::start().await;
    mount(
        &server,
        "/v1/chat/completions",
        json_response(
            200,
            &quench_serde::json!({"choices": [{"message": {"content": "{\"ok\":true}"}}]}),
        ),
    )
    .await;

    let text = VllmClient::new()
        .chat_json(
            // the scheme is tolerated and stripped
            "http://127.0.0.1",
            port_of(&server),
            "m",
            user("plan"),
            "search_plan",
            quench_serde::json!({"type": "object"}),
            50,
        )
        .await
        .expect("a completion");
    assert_eq!(text, "{\"ok\":true}");

    let sent = server.received_requests().await.expect("recorded");
    let body: quench_serde::Value = quench_serde::json::from_slice(&sent[0].body).expect("json");
    assert_eq!(body["model"], "m");
    assert_eq!(body["stream"], false);
    assert_eq!(body["max_tokens"], 50);
    assert_eq!(body["response_format"]["json_schema"]["name"], "search_plan");
}

#[quench_test::test]
async fn chat_json_reports_a_refusal_a_missing_content_and_an_unreachable_host() {
    let server = MockServer::start().await;
    mount(
        &server,
        "/v1/chat/completions",
        ResponseTemplate::new(500).set_body_raw("boom".to_string(), "text/plain"),
    )
    .await;
    let client = VllmClient::new();
    let err = client
        .chat_json(
            "127.0.0.1",
            port_of(&server),
            "m",
            user("x"),
            "s",
            quench_serde::json!({}),
            10,
        )
        .await
        .expect_err("a 500");
    assert!(err.to_string().contains("500"), "{err}");
    assert!(err.to_string().contains("boom"), "{err}");

    let empty = MockServer::start().await;
    mount(
        &empty,
        "/v1/chat/completions",
        json_response(200, &quench_serde::json!({"choices": []})),
    )
    .await;
    let err = VllmClient::new()
        .chat_json(
            "127.0.0.1",
            port_of(&empty),
            "m",
            user("x"),
            "s",
            quench_serde::json!({}),
            10,
        )
        .await
        .expect_err("no content");
    assert!(err.to_string().contains("no message content"), "{err}");

    // Nothing listens on the port of a server that has been dropped.
    let gone = {
        let server = MockServer::start().await;
        port_of(&server)
    };
    let err = VllmClient::new()
        .chat_json(
            "127.0.0.1",
            gone,
            "m",
            user("x"),
            "s",
            quench_serde::json!({}),
            10,
        )
        .await
        .expect_err("unreachable");
    assert!(err.to_string().contains("Failed to connect"), "{err}");
}

#[quench_test::test]
async fn repeated_failures_open_the_circuit_breaker() {
    let server = MockServer::start().await;
    mount(
        &server,
        "/v1/chat/completions",
        ResponseTemplate::new(503).set_body_raw("down".to_string(), "text/plain"),
    )
    .await;
    let client = VllmClient::new();
    for _ in 0..3 {
        let err = client
            .chat_json(
                "127.0.0.1",
                port_of(&server),
                "m",
                user("x"),
                "s",
                quench_serde::json!({}),
                10,
            )
            .await
            .expect_err("503");
        assert!(err.to_string().contains("503"), "{err}");
    }
    let before = server.received_requests().await.expect("recorded").len();

    let err = client
        .chat_json(
            "127.0.0.1",
            port_of(&server),
            "m",
            user("x"),
            "s",
            quench_serde::json!({}),
            10,
        )
        .await
        .expect_err("open breaker");
    assert!(err.to_string().contains("Circuit breaker open"), "{err}");
    let err = client
        .embeddings("127.0.0.1", port_of(&server), "m", vec!["a".to_string()])
        .await
        .expect_err("open breaker");
    assert!(err.to_string().contains("Circuit breaker open"), "{err}");
    let err = client
        .chat_stream("127.0.0.1", port_of(&server), "m", user("x"), None)
        .await
        .err()
        .expect("open breaker");
    assert!(err.to_string().contains("Circuit breaker open"), "{err}");
    // None of the refused calls reached the server.
    assert_eq!(
        server.received_requests().await.expect("recorded").len(),
        before
    );
}

#[quench_test::test]
async fn embeddings_come_back_in_input_order() {
    let server = MockServer::start().await;
    mount(
        &server,
        "/v1/embeddings",
        json_response(
            200,
            &quench_serde::json!({"data": [
                {"index": 1, "embedding": [0.0, 1.0]},
                {"index": 0, "embedding": [1.0, 0.0]},
            ]}),
        ),
    )
    .await;
    let vectors = VllmClient::new()
        .embeddings(
            "127.0.0.1",
            port_of(&server),
            "e",
            vec!["first".into(), "second".into()],
        )
        .await
        .expect("vectors");
    assert_eq!(vectors, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
}

#[quench_test::test]
async fn embeddings_reject_a_count_mismatch_a_refusal_and_a_garbled_body() {
    let server = MockServer::start().await;
    mount(
        &server,
        "/v1/embeddings",
        json_response(
            200,
            &quench_serde::json!({"data": [{"index": 0, "embedding": [1.0]}]}),
        ),
    )
    .await;
    let err = VllmClient::new()
        .embeddings(
            "127.0.0.1",
            port_of(&server),
            "e",
            vec!["a".into(), "b".into()],
        )
        .await
        .expect_err("one vector for two inputs");
    assert!(err.to_string().contains("1 embeddings for 2 inputs"), "{err}");

    let refused = MockServer::start().await;
    mount(
        &refused,
        "/v1/embeddings",
        ResponseTemplate::new(400).set_body_raw("bad input".to_string(), "text/plain"),
    )
    .await;
    let err = VllmClient::new()
        .embeddings("127.0.0.1", port_of(&refused), "e", vec!["a".into()])
        .await
        .expect_err("400");
    assert!(err.to_string().contains("bad input"), "{err}");

    let garbled = MockServer::start().await;
    mount(
        &garbled,
        "/v1/embeddings",
        ResponseTemplate::new(200).set_body_raw("not json".to_string(), "application/json"),
    )
    .await;
    let err = VllmClient::new()
        .embeddings("127.0.0.1", port_of(&garbled), "e", vec!["a".into()])
        .await
        .expect_err("garbled");
    assert!(err.to_string().contains("Failed to parse"), "{err}");
}

#[quench_test::test]
async fn chat_stream_yields_the_content_deltas_and_stops_at_done() {
    let server = MockServer::start().await;
    mount(
        &server,
        "/v1/chat/completions",
        sse_response(concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hel\"}}]}\n\n",
            "data: {not json}\n\n",
            "data: {\"choices\":[]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ignored\"}}]}\n\n",
        )),
    )
    .await;
    let stream = VllmClient::new()
        .chat_stream("127.0.0.1", port_of(&server), "m", user("hi"), Some(20))
        .await
        .expect("a stream");
    assert_eq!(collect(stream).await, vec!["Hel", "lo"]);

    let sent = server.received_requests().await.expect("recorded");
    let body: quench_serde::Value = quench_serde::json::from_slice(&sent[0].body).expect("json");
    assert_eq!(body["stream"], true);
    assert_eq!(body["max_tokens"], 20);
}

#[quench_test::test]
async fn chat_stream_turns_native_tool_calls_into_tool_call_text() {
    let server = MockServer::start().await;
    mount(
        &server,
        "/v1/chat/completions",
        sse_response(concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"file_list\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"dir\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"/\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        )),
    )
    .await;
    let stream = VllmClient::new()
        .chat_stream_with_tools(
            "127.0.0.1",
            port_of(&server),
            "m",
            user("list"),
            None,
            Some(vec![quench_serde::json!({"type": "function"})]),
        )
        .await
        .expect("a stream");
    assert_eq!(
        collect(stream).await,
        vec![r#"<tool_call>{"name": "file_list", "arguments": {"dir":"/"}}</tool_call>"#]
    );
}

#[quench_test::test]
async fn chat_stream_flushes_pending_tool_calls_when_the_stream_just_ends() {
    let server = MockServer::start().await;
    mount(
        &server,
        "/v1/chat/completions",
        // no finish_reason and no [DONE]
        sse_response(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"ping\"}}]}}]}\n\n",
        ),
    )
    .await;
    let stream = VllmClient::new()
        .chat_stream("127.0.0.1", port_of(&server), "m", user("x"), None)
        .await
        .expect("a stream");
    assert_eq!(
        collect(stream).await,
        vec![r#"<tool_call>{"name": "ping", "arguments": {}}</tool_call>"#]
    );
}

#[quench_test::test]
async fn chat_stream_surfaces_a_refusal() {
    let server = MockServer::start().await;
    mount(
        &server,
        "/v1/chat/completions",
        ResponseTemplate::new(429).set_body_raw("slow down".to_string(), "text/plain"),
    )
    .await;
    let err = VllmClient::new()
        .chat_stream("127.0.0.1", port_of(&server), "m", user("x"), None)
        .await
        .err()
        .expect("a 429");
    assert!(err.to_string().contains("slow down"), "{err}");
}
