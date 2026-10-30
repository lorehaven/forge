use sage_service::routers::turns::{
    embed_tool_results_into_response, html_escape, render_tool_result, stage_event,
};
use sage_service::tools::{ToolCall, ToolResult};

#[quench_test::test]
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

#[quench_test::test]
fn embed_tool_results_into_response_is_a_no_op_without_markers() {
    let response = "no tool calls here";
    assert_eq!(
        embed_tool_results_into_response(response, Vec::new()),
        response
    );
}

#[quench_test::test]
fn html_escape_escapes_every_special_character() {
    assert_eq!(
        html_escape("<a href=\"x\">it's & done</a>"),
        "&lt;a href=&quot;x&quot;&gt;it&#39;s &amp; done&lt;/a&gt;"
    );
}

#[quench_test::test]
fn render_tool_result_uses_the_search_icon_and_quoted_query_for_web_search() {
    let call = ToolCall {
        id: "1".to_string(),
        name: "web_search".to_string(),
        arguments: quench_serde::json!({"query": "rust async"}),
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

#[quench_test::test]
fn render_tool_result_escapes_error_content_and_marks_it_as_an_error() {
    let call = ToolCall {
        id: "1".to_string(),
        name: "calculator".to_string(),
        arguments: quench_serde::json!({}),
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

#[quench_test::test]
fn render_tool_result_converts_h1_headings_to_h3_in_markdown_output() {
    let call = ToolCall {
        id: "1".to_string(),
        name: "web_search".to_string(),
        arguments: quench_serde::json!({"query": "x"}),
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

#[quench_test::test]
fn stage_event_is_a_status_event_per_stage() {
    use sage_core::StreamEvent;
    use sage_service::grounding::Stage;
    let text = |stage: &Stage| String::from_utf8(stage_event(stage).to_vec()).unwrap();
    let parse = |frame: &str| {
        let name = frame
            .lines()
            .next()
            .unwrap()
            .strip_prefix("event: ")
            .unwrap();
        let data = frame
            .lines()
            .nth(1)
            .unwrap()
            .strip_prefix("data: ")
            .unwrap();
        StreamEvent::parse(name, data).unwrap()
    };
    let StreamEvent::Status(planning) = parse(&text(&Stage::Planning)) else {
        panic!()
    };
    assert_eq!(
        (planning.stage.as_str(), planning.detail),
        ("planning", None)
    );
    let StreamEvent::Status(searching) = parse(&text(&Stage::Searching(vec![
        "rust release".into(),
        "rust 1.99".into(),
    ]))) else {
        panic!()
    };
    assert_eq!(searching.stage, "searching");
    assert_eq!(
        searching.detail.as_deref(),
        Some("rust release · rust 1.99")
    );
    let StreamEvent::Status(reading) = parse(&text(&Stage::Reading(3))) else {
        panic!()
    };
    assert_eq!(reading.stage, "reading");
}
