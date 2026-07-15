use super::ToolCall;
use regex::Regex;

/// Matches either spelling of a tool-call opening tag (`<tool_call>` or
/// `<toolcall>`). Reused by parsing and stripping.
fn tool_tag_regex() -> Regex {
    Regex::new(r"(?s)<tool_?call>").unwrap()
}

/// Find the first balanced JSON object (`{...}`) in `s`, returning its byte
/// range. Brace counting skips braces that appear inside JSON strings and
/// honours backslash escapes, so nested objects and braces within string
/// values are handled correctly. Structural characters are ASCII, so byte
/// scanning is safe even when string contents are multi-byte UTF-8.
fn find_json_object(s: &str) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let start = s.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, &c) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((start, offset + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse tool calls from model output following the Qwen format:
/// Format 1: <tool_call>{"type": "function", "function": {"name": "web_search", "arguments": {"query": "..."}}}</tool_call>
/// Format 2: <toolcall>{"type": "search", "name": "websearch", "arguments": {"query": "..."}}</toolcall>
///
/// For each opening tag we extract the first balanced JSON object that follows
/// and parse that, rather than requiring the closing brace to sit right before
/// the closing tag. This tolerates trailing junk the model sometimes appends
/// (e.g. a stray `;` after the object), mismatched or missing closing tags, and
/// braces nested inside the arguments.
pub fn parse_tool_calls(content: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();

    tracing::debug!(
        "[PARSER] Attempting to parse tool calls from {} chars of content",
        content.len()
    );

    // Absolute offset past the last consumed JSON object, so a tag appearing
    // inside an already-parsed object is not treated as a new call.
    let mut consumed_until = 0;
    for m in tool_tag_regex().find_iter(content) {
        if m.end() < consumed_until {
            continue;
        }
        let rest = &content[m.end()..];
        let Some((s, e)) = find_json_object(rest) else {
            continue;
        };
        let json_str = &rest[s..e];
        tracing::debug!("[PARSER] Extracted JSON: {}", json_str);
        if let Some(call) = parse_tool_json_format1(json_str) {
            tracing::debug!("[PARSER] Successfully parsed as format1: {}", call.name);
            calls.push(call);
        } else if let Some(call) = parse_tool_json_format2(json_str) {
            tracing::debug!("[PARSER] Successfully parsed as format2: {}", call.name);
            calls.push(call);
        } else {
            tracing::warn!("[PARSER] Failed to parse JSON: {}", json_str);
        }
        consumed_until = m.end() + e;
    }

    tracing::info!("[PARSER] Total tool calls parsed: {}", calls.len());
    calls
}

/// Parse OpenAI function calling format: {"type": "function", "function": {"name": "...", "arguments": {...}}}
fn parse_tool_json_format1(json_str: &str) -> Option<ToolCall> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str)
        && let (Some(name), Some(args)) = (
            json["function"]["name"].as_str(),
            json["function"]["arguments"].as_object(),
        )
    {
        return Some(ToolCall {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            arguments: serde_json::Value::Object(args.clone()),
        });
    }
    None
}

/// Parse Qwen format: {"type": "search", "name": "websearch", "arguments": {...}}
fn parse_tool_json_format2(json_str: &str) -> Option<ToolCall> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
        // Try "name" field first (Qwen format)
        let tool_name = json["name"]
            .as_str()
            .or_else(|| json["function"]["name"].as_str());

        let args = json["arguments"]
            .as_object()
            .or_else(|| json["function"]["arguments"].as_object());

        if let (Some(name), Some(args_obj)) = (tool_name, args) {
            let normalized_name = normalize_tool_name(name);

            return Some(ToolCall {
                id: uuid::Uuid::new_v4().to_string(),
                name: normalized_name,
                arguments: serde_json::Value::Object(args_obj.clone()),
            });
        }
    }
    None
}

/// Normalize tool names to handle variations
fn normalize_tool_name(name: &str) -> String {
    match name {
        // Web search variations
        "websearch" | "web_search" | "search" => "web_search".to_string(),
        // Web fetch variations
        "webfetch" | "web_fetch" | "fetch" => "web_fetch".to_string(),
        // Calculator variations
        "calc" | "calculator" | "math" => "calculator".to_string(),
        // File ops variations
        "file_ops" | "fileops" | "files" => "file_ops".to_string(),
        // Command variations
        "cmd" | "command" | "shell" | "bash" => "command".to_string(),
        // Default: keep as-is
        other => other.to_string(),
    }
}

/// Remove tool call syntax from content to get clean display text. For each
/// opening tag we drop the tag, the balanced JSON object that follows, any
/// trailing punctuation/whitespace (e.g. a stray `;`), and the closing tag when
/// present. Residual bare tags (with no JSON) are stripped last.
pub fn strip_tool_calls(content: &str) -> String {
    // Removes optional trailing junk then a closing tag right after the object.
    let close_re = Regex::new(r"(?s)^[;,\s]*</tool_?call>").unwrap();

    let mut result = String::new();
    let mut last = 0;
    let mut consumed_until = 0;
    for m in tool_tag_regex().find_iter(content) {
        if m.start() < consumed_until {
            continue;
        }
        let rest = &content[m.end()..];
        let Some((_s, e)) = find_json_object(rest) else {
            continue;
        };
        let mut remove_end = m.end() + e;
        if let Some(cm) = close_re.find(&content[remove_end..]) {
            remove_end += cm.end();
        }
        result.push_str(&content[last..m.start()]);
        last = remove_end;
        consumed_until = remove_end;
    }
    result.push_str(&content[last..]);

    // Drop any leftover bare tags that were not part of a JSON construct.
    if let Ok(re) = Regex::new(r"</?tool_?call>") {
        result = re.replace_all(&result, "").to_string();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
