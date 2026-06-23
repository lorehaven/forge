use super::{ToolCall, ToolDefinition, ToolExecutor, ToolParameters, ToolResult};
use async_trait::async_trait;
use serde_json::json;

pub fn get_definition() -> ToolDefinition {
    ToolDefinition {
        name: "command".to_string(),
        description: "Execute shell commands (read-only operations only: grep, cat, ls, echo, pwd, date, etc.)".to_string(),
        tool_type: "function".to_string(),
        parameters: ToolParameters {
            param_type: "object".to_string(),
            properties: json!({
                "cmd": {
                    "type": "string",
                    "description": "Shell command to execute (limited to safe read-only commands)"
                }
            }),
            required: vec!["cmd".to_string()],
        },
    }
}

pub struct CommandExecutor {
    last_execution: std::sync::Mutex<std::time::SystemTime>,
    execution_count: std::sync::atomic::AtomicU32,
}

impl CommandExecutor {
    pub fn new() -> Self {
        Self {
            last_execution: std::sync::Mutex::new(std::time::SystemTime::now()),
            execution_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn check_rate_limit(&self) -> Result<(), String> {
        let count = self.execution_count.load(std::sync::atomic::Ordering::Relaxed);
        if count >= 10 {
            // Max 10 commands per execution session
            return Err("Command rate limit exceeded (max 10 commands per session)".to_string());
        }

        if let Ok(last) = self.last_execution.lock() {
            if let Ok(elapsed) = last.elapsed() {
                // Max 5 commands per second
                if elapsed.as_millis() < 200 {
                    return Err("Command rate limit exceeded (max 5/sec)".to_string());
                }
            }
        }

        Ok(())
    }
}

impl Default for CommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for CommandExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        // Check rate limit
        if let Err(msg) = self.check_rate_limit() {
            return ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: msg,
                is_error: true,
            };
        }

        // Update execution tracking
        self.execution_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut last) = self.last_execution.lock() {
            *last = std::time::SystemTime::now();
        }
        let cmd = match tool_call.arguments.get("cmd") {
            Some(val) => match val.as_str() {
                Some(s) => s.to_string(),
                None => {
                    return ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: "Invalid command: must be a string".to_string(),
                        is_error: true,
                    }
                }
            },
            None => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: "Missing 'cmd' argument".to_string(),
                    is_error: true,
                }
            }
        };

        // Security: only allow safe, read-only commands
        if !is_safe_command(&cmd) {
            return ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: "Command not allowed: only read-only operations permitted (cat, grep, ls, echo, pwd, date, wc, head, tail, etc.)".to_string(),
                is_error: true,
            };
        }

        match execute_command(&cmd).await {
            Ok(output) => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: output,
                is_error: false,
            },
            Err(err) => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: format!("Command failed: {}", err),
                is_error: true,
            },
        }
    }
}

fn is_safe_command(cmd: &str) -> bool {
    let cmd_lower = cmd.trim().to_lowercase();

    // Whitelist of safe commands
    let safe_commands = [
        "cat", "grep", "ls", "find", "pwd", "date", "echo", "wc", "head", "tail",
        "sort", "uniq", "cut", "tr", "sed", "awk", "stat", "file", "which", "type",
        "du", "df", "ps", "env", "whoami", "id", "uname", "uptime", "free",
    ];

    // Get the first word of the command
    let first_word = cmd_lower.split_whitespace().next().unwrap_or("");

    // Blacklist dangerous patterns
    let dangerous_patterns = [
        "rm ", "mv ", "cp ", ">", ">>", "|", "&", ";", "sudo", "su ",
        "chmod", "chown", "mkfs", "dd", "shutdown", "reboot", "kill", "pkill",
    ];

    // Check for dangerous patterns
    for pattern in &dangerous_patterns {
        if cmd_lower.contains(pattern) {
            return false;
        }
    }

    // Check if command is in whitelist
    safe_commands.contains(&first_word)
}

async fn execute_command(cmd: &str) -> Result<String, String> {
    let output = if cfg!(target_os = "windows") {
        tokio::process::Command::new("cmd")
            .args(&["/C", cmd])
            .output()
            .await
    } else {
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .await
    };

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                // Limit output to 2000 characters
                let mut result = stdout.to_string();
                if result.len() > 2000 {
                    result.truncate(2000);
                    result.push_str("\n[Output truncated...]");
                }
                Ok(result)
            } else {
                let error_msg = if stderr.is_empty() {
                    format!("Command failed with exit code: {}", output.status.code().unwrap_or(-1))
                } else {
                    stderr.to_string()
                };
                Err(error_msg)
            }
        }
        Err(e) => Err(format!("Failed to execute command: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_commands() {
        assert!(is_safe_command("cat /etc/hostname"));
        assert!(is_safe_command("ls -la"));
        assert!(is_safe_command("grep pattern file.txt"));
        assert!(is_safe_command("pwd"));
        assert!(is_safe_command("date"));
    }

    #[test]
    fn test_dangerous_commands() {
        assert!(!is_safe_command("rm -rf /"));
        assert!(!is_safe_command("sudo cat file"));
        assert!(!is_safe_command("cat file > /tmp/out"));
        assert!(!is_safe_command("kill -9 1234"));
        assert!(!is_safe_command("dd if=/dev/zero of=/dev/sda"));
    }
}
