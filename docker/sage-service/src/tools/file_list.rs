use crate::files::STATUS_READY;
use crate::models::Conversation;
use crate::tools::{ToolCall, ToolDefinition, ToolExecutor, ToolParameters, ToolResult};
use quench_db::prelude::{Crud, Db};

pub fn get_definition() -> ToolDefinition {
    ToolDefinition {
        name: "file_list".to_string(),
        description: "List the files the user uploaded to this conversation or its project. \
                      Returns each file's name, type, size and processing status. Use this to \
                      see what documents are available before searching their content with \
                      file_search."
            .to_string(),
        tool_type: "function".to_string(),
        parameters: ToolParameters {
            param_type: "object".to_string(),
            properties: serde_json::json!({
                "ready_only": {
                    "type": "boolean",
                    "description": "When true, only list files that finished processing and \
                                    are searchable (default false)."
                }
            }),
            required: vec![],
        },
    }
}

pub struct FileListExecutor {
    db: Db,
    conversation_id: Option<String>,
    /// The request's project scope, used to list project files when the
    /// conversation row does not exist yet (e.g. the first message in a
    /// project, where the conversation is persisted only after tools run).
    project_id: Option<String>,
}

impl FileListExecutor {
    pub fn new(db: Db, conversation_id: Option<String>, project_id: Option<String>) -> Self {
        Self {
            db,
            conversation_id,
            project_id,
        }
    }
}

/// Human-readable byte size for a file listing line.
fn format_size(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

#[async_trait::async_trait]
impl ToolExecutor for FileListExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let Some(conversation_id) = &self.conversation_id else {
            return ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: "file_list is only available within a conversation".to_string(),
                is_error: true,
            };
        };

        let ready_only = tool_call
            .arguments
            .get("ready_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Prefer the conversation's view (its own files plus its project's).
        // The conversation may not be persisted yet on the first message of a
        // project chat, so fall back to the request's project scope.
        let files_result = match self
            .db
            .repository::<Conversation>()
            .read(conversation_id)
            .await
        {
            Ok(Some(c)) => {
                crate::routers::files::visible_files_for_conversation(&self.db, &c).await
            }
            Ok(None) => match &self.project_id {
                Some(pid) => crate::routers::files::visible_files_for_project(&self.db, pid).await,
                None => {
                    return ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: "No files have been uploaded yet.".to_string(),
                        is_error: false,
                    };
                }
            },
            Err(e) => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: format!("Failed to load conversation: {}", e),
                    is_error: true,
                };
            }
        };

        let files = match files_result {
            Ok(files) => files,
            Err(e) => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: format!("Failed to list files: {}", e),
                    is_error: true,
                };
            }
        };

        let files: Vec<_> = files
            .into_iter()
            .filter(|f| !ready_only || f.status == STATUS_READY)
            .collect();

        if files.is_empty() {
            return ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: "No files have been uploaded to this conversation or its project."
                    .to_string(),
                is_error: false,
            };
        }

        let mut content = format!("{} uploaded file(s):\n\n", files.len());
        for file in &files {
            content.push_str(&format!(
                "- {} ({}, {}) — {}\n",
                file.file_name,
                file.mime_type,
                format_size(file.file_size),
                file.status
            ));
        }
        content.push_str("\nUse the file_search tool to search the content of the ready files.");

        ToolResult {
            tool_use_id: tool_call.id.clone(),
            content,
            is_error: false,
        }
    }
}
