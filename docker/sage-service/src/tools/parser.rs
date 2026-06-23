use super::ToolCall;
use regex::Regex;

/// Parse tool calls from model output following the Qwen format:
/// Format 1: <tool_call>{"type": "function", "function": {"name": "web_search", "arguments": {"query": "..."}}</tool_call>
/// Format 2: <toolcall>{"type": "search", "name": "websearch", "arguments": {"query": "..."}}</toolcall>
pub fn parse_tool_calls(content: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();

    tracing::debug!(
        "[PARSER] Attempting to parse tool calls from {} chars of content",
        content.len()
    );

    // Try multiple regex patterns to catch different tool call formats
    // Note: using (?s) flag for DOTALL to match newlines with .
    let patterns = [
        r"(?s)<tool_call>\s*(\{.*?\})\s*</tool_call>",
        // Pattern 2: <toolcall>...</toolcall>
        r"(?s)<toolcall>\s*(\{.*?\})\s*</toolcall>",
        // Pattern 3: <toolcall>...</tool_call> (mismatched tags - Qwen sometimes does this)
        r"(?s)<toolcall>\s*(\{.*?\})\s*</tool_call>",
        // Pattern 4: <tool_call>...</toolcall> (reverse mismatch)
        r"(?s)<tool_call>\s*(\{.*?\})\s*</toolcall>",
        // Pattern 5: <toolcall>{...} (unclosed - model sometimes forgets closing tag)
        r"(?s)<toolcall>\s*(\{[^<]*?\})\s*(?:</toolcall>|(?=\n|$))",
        // Pattern 6: <tool_call>{...} (unclosed variant)
        r"(?s)<tool_call>\s*(\{[^<]*?\})\s*(?:</tool_call>|(?=\n|$))",
    ];

    for (idx, pattern) in patterns.iter().enumerate() {
        if let Ok(re) = Regex::new(pattern) {
            let matches: Vec<_> = re.captures_iter(content).collect();
            if !matches.is_empty() {
                tracing::debug!("[PARSER] Pattern {} matched {} times", idx, matches.len());
                for cap in matches {
                    let json_str = &cap[1];
                    tracing::debug!("[PARSER] Pattern {} JSON: {}", idx, json_str);
                    // Try both parsing formats since we don't know which one it is
                    if let Some(call) = parse_tool_json_format1(json_str) {
                        tracing::debug!("[PARSER] Successfully parsed as format1: {}", call.name);
                        calls.push(call);
                    } else if let Some(call) = parse_tool_json_format2(json_str) {
                        tracing::debug!("[PARSER] Successfully parsed as format2: {}", call.name);
                        calls.push(call);
                    } else {
                        tracing::warn!("[PARSER] Failed to parse JSON: {}", json_str);
                    }
                }
            }
        }
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
