use super::{ToolCall, ToolDefinition, ToolExecutor, ToolParameters, ToolResult};
use async_trait::async_trait;
use serde_json::json;

#[must_use]
pub fn get_definition() -> ToolDefinition {
    ToolDefinition {
        name: "file_ops".to_string(),
        description: "Read, write, and list files. Available operations: read, write, list, exists"
            .to_string(),
        tool_type: "function".to_string(),
        parameters: ToolParameters {
            param_type: "object".to_string(),
            properties: json!({
                "operation": {
                    "type": "string",
                    "enum": ["read", "write", "list", "exists"],
                    "description": "Operation to perform"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write (only for write operation)"
                }
            }),
            required: vec!["operation".to_string(), "path".to_string()],
        },
    }
}

#[derive(Debug)]
pub struct FileOpsExecutor {
    base_path: std::path::PathBuf,
    canonical_base: std::path::PathBuf,
}

impl FileOpsExecutor {
    #[must_use]
    pub fn new(base_path: String) -> Self {
        let path = std::path::PathBuf::from(base_path);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        Self {
            base_path: path,
            canonical_base: canonical,
        }
    }

    #[must_use]
    pub fn from_env() -> Self {
        let base_path = std::env::var("FILE_OPS_BASE_PATH").unwrap_or_else(|_| ".".to_string());
        Self::new(base_path)
    }

    fn is_safe(&self, path: &str) -> bool {
        // Reject absolute paths
        if std::path::Path::new(path).is_absolute() {
            return false;
        }

        // Reject paths with dangerous patterns
        let dangerous = ["../", "..\\", "\x00", "~"];
        if dangerous.iter().any(|p| path.contains(p)) {
            return false;
        }

        // Check symlink resolution
        let full_path = self.base_path.join(path);
        full_path.canonicalize().map_or_else(
            |_| {
                // For non-existent files, check parent
                full_path.parent().is_some_and(|parent| {
                    parent.canonicalize().is_ok_and(|canonical_parent| {
                        canonical_parent.starts_with(&self.canonical_base)
                    })
                })
            },
            |canonical| canonical.starts_with(&self.canonical_base),
        )
    }
}

impl Default for FileOpsExecutor {
    fn default() -> Self {
        Self::from_env()
    }
}

/// A result for `tool_call`: the output when `is_error` is false, the error otherwise.
fn outcome(tool_call: &ToolCall, content: impl Into<String>, is_error: bool) -> ToolResult {
    ToolResult {
        tool_use_id: tool_call.id.clone(),
        content: content.into(),
        is_error,
    }
}

/// The string argument `key`, or the error result to return when it is missing or not a string.
fn string_arg<'a>(
    tool_call: &'a ToolCall,
    key: &str,
    missing: &str,
) -> Result<&'a str, ToolResult> {
    tool_call.arguments.get(key).map_or_else(
        || Err(outcome(tool_call, missing, true)),
        |val| {
            val.as_str()
                .ok_or_else(|| outcome(tool_call, format!("Invalid {key}: must be a string"), true))
        },
    )
}

/// One line per entry (at most 50) of the directory, with its size or `/` for a directory.
fn list_entries(entries: std::fs::ReadDir) -> Vec<String> {
    entries
        .take(50)
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let is_dir = metadata.is_dir();
            let name = entry.file_name();
            Some(format!(
                "{}{} ({})",
                name.to_string_lossy(),
                if is_dir { "/" } else { "" },
                if is_dir {
                    "dir".to_string()
                } else {
                    format!("{} bytes", metadata.len())
                }
            ))
        })
        .collect()
}

#[async_trait]
impl ToolExecutor for FileOpsExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let operation = match string_arg(tool_call, "operation", "Missing 'operation' argument") {
            Ok(operation) => operation,
            Err(result) => return result,
        };
        let path = match string_arg(tool_call, "path", "Missing 'path' argument") {
            Ok(path) => path,
            Err(result) => return result,
        };

        // Security: ensure path is within base_path
        if !self.is_safe(path) {
            return outcome(
                tool_call,
                "Access denied: path is outside allowed directory or contains dangerous characters",
                true,
            );
        }

        let full_path = self.base_path.join(path);

        match operation {
            "read" => match std::fs::read_to_string(&full_path) {
                Ok(content) => outcome(tool_call, content, false),
                Err(e) => outcome(tool_call, format!("Failed to read file: {e}"), true),
            },
            "write" => {
                let content = match string_arg(
                    tool_call,
                    "content",
                    "Missing 'content' argument for write operation",
                ) {
                    Ok(content) => content,
                    Err(result) => return result,
                };

                match std::fs::write(&full_path, content) {
                    Ok(()) => outcome(tool_call, format!("Successfully wrote to {path}"), false),
                    Err(e) => outcome(tool_call, format!("Failed to write file: {e}"), true),
                }
            }
            "list" => match std::fs::read_dir(&full_path) {
                Ok(entries) => outcome(
                    tool_call,
                    format!("Files in {}:\n{}", path, list_entries(entries).join("\n")),
                    false,
                ),
                Err(e) => outcome(tool_call, format!("Failed to list directory: {e}"), true),
            },
            "exists" => {
                let exists = full_path.exists();
                outcome(
                    tool_call,
                    format!(
                        "{}: {}",
                        path,
                        if exists { "exists" } else { "does not exist" }
                    ),
                    false,
                )
            }
            _ => outcome(
                tool_call,
                format!(
                    "Unknown operation: {operation}. Must be one of: read, write, list, exists"
                ),
                true,
            ),
        }
    }
}
