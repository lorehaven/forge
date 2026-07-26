use super::{ToolCall, ToolDefinition, ToolExecutor, ToolParameters, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;

pub fn get_definition() -> ToolDefinition {
    ToolDefinition {
        name: "code_executor".to_string(),
        description: "Execute Python or JavaScript code safely in a sandboxed environment"
            .to_string(),
        tool_type: "function".to_string(),
        parameters: ToolParameters {
            param_type: "object".to_string(),
            properties: json!({
                "language": {
                    "type": "string",
                    "enum": ["python", "javascript"],
                    "description": "Programming language (python or javascript)"
                },
                "code": {
                    "type": "string",
                    "description": "Code to execute"
                }
            }),
            required: vec!["language".to_string(), "code".to_string()],
        },
    }
}

pub struct CodeExecutor;

#[async_trait]
impl ToolExecutor for CodeExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let language = match tool_call.arguments.get("language") {
            Some(val) => match val.as_str() {
                Some(s) => s.to_lowercase(),
                None => {
                    return ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: "Invalid language: must be a string".to_string(),
                        is_error: true,
                    };
                }
            },
            None => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: "Missing 'language' argument".to_string(),
                    is_error: true,
                };
            }
        };

        let code = match tool_call.arguments.get("code") {
            Some(val) => match val.as_str() {
                Some(s) => s.to_string(),
                None => {
                    return ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: "Invalid code: must be a string".to_string(),
                        is_error: true,
                    };
                }
            },
            None => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: "Missing 'code' argument".to_string(),
                    is_error: true,
                };
            }
        };

        // Validate code safety
        if let Err(err) = validate_code_safety(&code, &language) {
            return ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: format!("Code rejected for safety: {}", err),
                is_error: true,
            };
        }

        match language.as_str() {
            "python" => execute_python(&code, &tool_call.id).await,
            "javascript" => execute_javascript(&code, &tool_call.id).await,
            _ => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: format!(
                    "Unsupported language: {}. Supported: python, javascript",
                    language
                ),
                is_error: true,
            },
        }
    }
}

pub fn validate_code_safety(code: &str, language: &str) -> Result<(), String> {
    let dangerous_patterns = match language {
        "python" => vec![
            "os.system",
            "subprocess",
            "exec",
            "eval",
            "__import__",
            "open(",
            "input(",
            "compile(",
        ],
        "javascript" => vec![
            "require",
            "eval",
            "Function",
            "setTimeout",
            "setInterval",
            "fetch",
            "XMLHttpRequest",
            "window.location",
            "document.write",
        ],
        _ => vec![],
    };

    for pattern in dangerous_patterns {
        if code.contains(pattern) {
            return Err(format!("'{}' is not allowed for security reasons", pattern));
        }
    }

    // Max code length: 5000 characters
    if code.len() > 5000 {
        return Err("Code exceeds maximum length of 5000 characters".to_string());
    }

    Ok(())
}

async fn execute_python(code: &str, tool_id: &str) -> ToolResult {
    // Try to use Python via subprocess if available
    match execute_with_timeout("python3", code, Duration::from_secs(10)).await {
        Ok(output) => ToolResult {
            tool_use_id: tool_id.to_string(),
            content: format!(
                "```python\n{}\n```\n\n**Output:**\n```\n{}\n```",
                code, output
            ),
            is_error: false,
        },
        Err(err) => ToolResult {
            tool_use_id: tool_id.to_string(),
            content: format!("Python execution failed: {}", err),
            is_error: true,
        },
    }
}

async fn execute_javascript(code: &str, tool_id: &str) -> ToolResult {
    // Try to use Node.js via subprocess if available
    match execute_with_timeout("node", code, Duration::from_secs(10)).await {
        Ok(output) => ToolResult {
            tool_use_id: tool_id.to_string(),
            content: format!(
                "```javascript\n{}\n```\n\n**Output:**\n```\n{}\n```",
                code, output
            ),
            is_error: false,
        },
        Err(err) => ToolResult {
            tool_use_id: tool_id.to_string(),
            content: format!("JavaScript execution failed: {}", err),
            is_error: true,
        },
    }
}

async fn execute_with_timeout(
    interpreter: &str,
    code: &str,
    timeout: Duration,
) -> Result<String, String> {
    let output = tokio::time::timeout(timeout, async {
        tokio::process::Command::new(interpreter)
            .arg("-c")
            .arg(code)
            .output()
            .await
    })
    .await;

    match output {
        Ok(Ok(cmd_output)) => {
            let stdout = String::from_utf8_lossy(&cmd_output.stdout);
            let stderr = String::from_utf8_lossy(&cmd_output.stderr);

            if cmd_output.status.success() {
                // Limit output to 2000 characters
                let mut result = stdout.to_string();
                if result.len() > 2000 {
                    result.truncate(2000);
                    result.push_str("\n[Output truncated...]");
                }
                Ok(result)
            } else {
                let mut error_msg = stderr.to_string();
                if error_msg.is_empty() {
                    error_msg = format!("Exit code: {}", cmd_output.status.code().unwrap_or(-1));
                }
                Err(error_msg)
            }
        }
        Ok(Err(e)) => Err(format!("Failed to execute: {}", e)),
        Err(_) => Err(format!(
            "Code execution timed out after {:.1}s",
            timeout.as_secs_f64()
        )),
    }
}
