use super::ToolCall;
use regex::Regex;

/// Parse tool calls from model output following the Qwen format:
/// Format 1: <tool_call>{"type": "function", "function": {"name": "web_search", "arguments": {"query": "..."}}</tool_call>
/// Format 2: <toolcall>{"type": "search", "name": "websearch", "arguments": {"query": "..."}}</toolcall>
pub fn parse_tool_calls(content: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();

    // Try multiple regex patterns to catch different tool call formats
    // Pattern 1: <tool_call>...</tool_call>
    let patterns = [
        r"<tool_call>\s*(\{.*?\})\s*</tool_call>",
        // Pattern 2: <toolcall>...</toolcall>
        r"<toolcall>\s*(\{.*?\})\s*</toolcall>",
        // Pattern 3: <toolcall>...</tool_call> (mismatched tags - Qwen sometimes does this)
        r"<toolcall>\s*(\{.*?\})\s*</tool_call>",
        // Pattern 4: <tool_call>...</toolcall> (reverse mismatch)
        r"<tool_call>\s*(\{.*?\})\s*</toolcall>",
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            for cap in re.captures_iter(content) {
                // Try both parsing formats since we don't know which one it is
                if let Some(call) = parse_tool_json_format1(&cap[1]) {
                    calls.push(call);
                } else if let Some(call) = parse_tool_json_format2(&cap[1]) {
                    calls.push(call);
                }
            }
        }
    }

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
            // Map "websearch" -> "web_search" for consistency
            let normalized_name = if name == "websearch" {
                "web_search".to_string()
            } else {
                name.to_string()
            };

            return Some(ToolCall {
                id: uuid::Uuid::new_v4().to_string(),
                name: normalized_name,
                arguments: serde_json::Value::Object(args_obj.clone()),
            });
        }
    }
    None
}

/// Remove tool call XML tags from content to get clean text
pub fn strip_tool_calls(content: &str) -> String {
    let mut result = content.to_string();

    // Strip all possible formats
    let patterns = [
        r"<tool_call>\s*(\{.*?\})\s*</tool_call>",
        r"<toolcall>\s*(\{.*?\})\s*</toolcall>",
        r"<toolcall>\s*(\{.*?\})\s*</tool_call>", // Mismatched
        r"<tool_call>\s*(\{.*?\})\s*</toolcall>", // Reverse mismatch
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            result = re.replace_all(&result, "").to_string();
        }
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
