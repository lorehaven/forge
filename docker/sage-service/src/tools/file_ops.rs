use super::{ToolCall, ToolDefinition, ToolExecutor, ToolParameters, ToolResult};
use async_trait::async_trait;
use serde_json::json;

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

pub struct FileOpsExecutor {
    base_path: std::path::PathBuf,
    canonical_base: std::path::PathBuf,
}

impl FileOpsExecutor {
    pub fn new(base_path: String) -> Self {
        let path = std::path::PathBuf::from(base_path);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        Self {
            base_path: path,
            canonical_base: canonical,
        }
    }

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
        match full_path.canonicalize() {
            Ok(canonical) => canonical.starts_with(&self.canonical_base),
            Err(_) => {
                // For non-existent files, check parent
                if let Some(parent) = full_path.parent() {
                    match parent.canonicalize() {
                        Ok(canonical_parent) => canonical_parent.starts_with(&self.canonical_base),
                        Err(_) => false,
                    }
                } else {
                    false
                }
            }
        }
    }
}

impl Default for FileOpsExecutor {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait]
impl ToolExecutor for FileOpsExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let operation = match tool_call.arguments.get("operation") {
            Some(val) => match val.as_str() {
                Some(s) => s,
                None => {
                    return ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: "Invalid operation: must be a string".to_string(),
                        is_error: true,
                    };
                }
            },
            None => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: "Missing 'operation' argument".to_string(),
                    is_error: true,
                };
            }
        };

        let path = match tool_call.arguments.get("path") {
            Some(val) => match val.as_str() {
                Some(s) => s,
                None => {
                    return ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: "Invalid path: must be a string".to_string(),
                        is_error: true,
                    };
                }
            },
            None => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: "Missing 'path' argument".to_string(),
                    is_error: true,
                };
            }
        };

        // Security: ensure path is within base_path
        if !self.is_safe(path) {
            return ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: "Access denied: path is outside allowed directory or contains dangerous characters".to_string(),
                is_error: true,
            };
        }

        let full_path = self.base_path.join(path);

        match operation {
            "read" => match std::fs::read_to_string(&full_path) {
                Ok(content) => ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content,
                    is_error: false,
                },
                Err(e) => ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: format!("Failed to read file: {}", e),
                    is_error: true,
                },
            },
            "write" => {
                let content = match tool_call.arguments.get("content") {
                    Some(val) => match val.as_str() {
                        Some(s) => s,
                        None => {
                            return ToolResult {
                                tool_use_id: tool_call.id.clone(),
                                content: "Invalid content: must be a string".to_string(),
                                is_error: true,
                            };
                        }
                    },
                    None => {
                        return ToolResult {
                            tool_use_id: tool_call.id.clone(),
                            content: "Missing 'content' argument for write operation".to_string(),
                            is_error: true,
                        };
                    }
                };

                match std::fs::write(&full_path, content) {
                    Ok(_) => ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: format!("Successfully wrote to {}", path),
                        is_error: false,
                    },
                    Err(e) => ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: format!("Failed to write file: {}", e),
                        is_error: true,
                    },
                }
            }
            "list" => match std::fs::read_dir(&full_path) {
                Ok(entries) => {
                    let mut files = Vec::new();
                    for entry in entries.take(50).flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            let is_dir = metadata.is_dir();
                            let name = entry.file_name();
                            let name_str = name.to_string_lossy();
                            files.push(format!(
                                "{}{} ({})",
                                name_str,
                                if is_dir { "/" } else { "" },
                                if is_dir {
                                    "dir".to_string()
                                } else {
                                    format!("{} bytes", metadata.len())
                                }
                            ));
                        }
                    }

                    ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: format!("Files in {}:\n{}", path, files.join("\n")),
                        is_error: false,
                    }
                }
                Err(e) => ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: format!("Failed to list directory: {}", e),
                    is_error: true,
                },
            },
            "exists" => {
                let exists = full_path.exists();
                ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: format!(
                        "{}: {}",
                        path,
                        if exists { "exists" } else { "does not exist" }
                    ),
                    is_error: false,
                }
            }
            _ => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: format!(
                    "Unknown operation: {}. Must be one of: read, write, list, exists",
                    operation
                ),
                is_error: true,
            },
        }
    }
}
