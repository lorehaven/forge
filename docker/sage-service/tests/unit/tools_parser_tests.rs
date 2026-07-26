//! Unit tests for `tools/parser.rs`.

use sage_service::tools::parser::*;

#[test]
fn test_parse_tool_calls_format1() {
    let content = r#"Let me search for that information.
<tool_call>
{"type": "function", "function": {"name": "web_search", "arguments": {"query": "latest Rust releases"}}}
</tool_call>
Found some results."#;

    let calls = parse_tool_calls(content);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "web_search");
    assert_eq!(calls[0].arguments["query"], "latest Rust releases");
}

#[test]
fn test_parse_tool_calls_format2() {
    let content = r#"Let me search for that information.
<toolcall>
{"type": "search", "name": "websearch", "arguments": {"query": "latest Python releases"}}
</toolcall>
Found some results."#;

    let calls = parse_tool_calls(content);
    assert_eq!(calls.len(), 1);
    // Should normalize "websearch" to "web_search"
    assert_eq!(calls[0].name, "web_search");
    assert_eq!(calls[0].arguments["query"], "latest Python releases");
}

#[test]
fn test_parse_tool_calls_mismatched_tags() {
    // Test the Qwen quirk: <toolcall>...</tool_call> (mismatched)
    let content = r#"Let me search.
<toolcall> {"type": "search", "name": "websearch", "arguments": {"query": "Python 2026"}} </tool_call>
Found it."#;

    let calls = parse_tool_calls(content);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "web_search");
    assert_eq!(calls[0].arguments["query"], "Python 2026");
}

#[test]
fn test_parse_tool_calls_trailing_semicolon() {
    // Regression: the model appended a stray `;` after the JSON object,
    // which defeated the old "closing brace must abut the closing tag"
    // regexes and broke tool calling entirely.
    let content = r#"Sure, let me list the files.
<toolcall>{"name": "file_list", "arguments": {}};</toolcall>
"#;

    let calls = parse_tool_calls(content);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "file_list");
    assert!(calls[0].arguments.as_object().unwrap().is_empty());
}

#[test]
fn test_parse_tool_calls_nested_braces_and_no_closing_tag() {
    // Nested object in arguments, no closing tag at all.
    let content =
        r#"<tool_call>{"name": "file_search", "arguments": {"query": "a {b} c", "opts": {}}}"#;

    let calls = parse_tool_calls(content);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "file_search");
    assert_eq!(calls[0].arguments["query"], "a {b} c");
}

#[test]
fn test_strip_tool_calls_trailing_semicolon() {
    let content = r#"Before.
<toolcall>{"name": "file_list", "arguments": {}};</toolcall>
After."#;
    let cleaned = strip_tool_calls(content);
    assert!(!cleaned.contains("<toolcall>"));
    assert!(!cleaned.contains("</toolcall>"));
    assert!(!cleaned.contains("file_list"));
    assert!(!cleaned.contains(';'));
    assert!(cleaned.contains("Before."));
    assert!(cleaned.contains("After."));
}

#[test]
fn test_strip_tool_calls() {
    let content = r#"Let me search.
<tool_call>
{"type": "function", "function": {"name": "web_search", "arguments": {"query": "test"}}}
</tool_call>
And also:
<toolcall>
{"type": "search", "name": "websearch", "arguments": {"query": "another"}}
</toolcall>
And mismatched:
<toolcall> {"type": "search", "name": "websearch", "arguments": {"query": "Python"}} </tool_call>
Here are results."#;

    let cleaned = strip_tool_calls(content);
    assert!(!cleaned.contains("<tool_call>"));
    assert!(!cleaned.contains("<toolcall>"));
    assert!(!cleaned.contains("</tool_call>"));
    assert!(cleaned.contains("Let me search"));
    assert!(cleaned.contains("Here are results"));
    assert!(cleaned.contains("And also"));
    assert!(cleaned.contains("And mismatched"));
}
