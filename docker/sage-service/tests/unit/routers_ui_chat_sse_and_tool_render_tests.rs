use sage_service::routers::ui::chat::{
    embed_tool_results_into_response, encode_sse, html_escape, render_tool_result,
};
use sage_service::tools::{ToolCall, ToolResult};

#[test]
fn encode_sse_frames_each_line_of_multiline_data_with_its_own_data_prefix() {
    let bytes = encode_sse("message", "line one\nline two");
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert_eq!(text, "event: message\ndata: line one\ndata: line two\n\n");
}

#[test]
fn embed_tool_results_into_response_replaces_every_marker_with_its_result() {
    let response = "before <toolcall>{\"name\":\"web_search\"}</toolcall> after";
    let markers = vec![(
        "<toolcall>{\"name\":\"web_search\"}</toolcall>".to_string(),
        "<div>result</div>".to_string(),
    )];
    let embedded = embed_tool_results_into_response(response, markers);
    assert!(embedded.contains("<div>result</div>"));
    assert!(!embedded.contains("<toolcall>"));
    assert!(embedded.starts_with("before"));
    assert!(embedded.trim_end().ends_with("after"));
}

#[test]
fn embed_tool_results_into_response_is_a_no_op_without_markers() {
    let response = "no tool calls here";
    assert_eq!(
        embed_tool_results_into_response(response, Vec::new()),
        response
    );
}

#[test]
fn html_escape_escapes_every_special_character() {
    assert_eq!(
        html_escape("<a href=\"x\">it's & done</a>"),
        "&lt;a href=&quot;x&quot;&gt;it&#39;s &amp; done&lt;/a&gt;"
    );
}

#[test]
fn render_tool_result_uses_the_search_icon_and_quoted_query_for_web_search() {
    let call = ToolCall {
        id: "1".to_string(),
        name: "web_search".to_string(),
        arguments: serde_json::json!({"query": "rust async"}),
    };
    let result = ToolResult {
        tool_use_id: "1".to_string(),
        content: "some results".to_string(),
        is_error: false,
    };
    let html = render_tool_result(&call, &result);
    assert!(html.contains("🔍"));
    assert!(html.contains("rust async"));
    assert!(html.contains("tool-success"));
}

#[test]
fn render_tool_result_escapes_error_content_and_marks_it_as_an_error() {
    let call = ToolCall {
        id: "1".to_string(),
        name: "calculator".to_string(),
        arguments: serde_json::json!({}),
    };
    let result = ToolResult {
        tool_use_id: "1".to_string(),
        content: "<script>bad</script>".to_string(),
        is_error: true,
    };
    let html = render_tool_result(&call, &result);
    assert!(html.contains("tool-error"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>bad"));
}

#[test]
fn render_tool_result_converts_h1_headings_to_h3_in_markdown_output() {
    let call = ToolCall {
        id: "1".to_string(),
        name: "web_search".to_string(),
        arguments: serde_json::json!({"query": "x"}),
    };
    let result = ToolResult {
        tool_use_id: "1".to_string(),
        content: "# Heading\n\nbody".to_string(),
        is_error: false,
    };
    let html = render_tool_result(&call, &result);
    assert!(html.contains("<h3>"));
    assert!(!html.contains("<h1>"));
}
