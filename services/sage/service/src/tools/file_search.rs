use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::VllmClient;
use crate::files::rag;
use crate::tools::{ToolCall, ToolDefinition, ToolExecutor, ToolParameters, ToolResult};
use quench_db::prelude::Db;
use std::fmt::Write as _;

#[must_use]
pub fn get_definition() -> ToolDefinition {
    ToolDefinition {
        name: "file_search".to_string(),
        description: "Semantically search the content of files the user uploaded to this \
                      conversation or its project. Returns the most relevant text excerpts \
                      with their source file names. Use this whenever the question may be \
                      answered by an uploaded document."
            .to_string(),
        tool_type: "function".to_string(),
        parameters: ToolParameters {
            param_type: "object".to_string(),
            properties: quench_serde::json!({
                "query": {
                    "type": "string",
                    "description": "What to search for in the uploaded files"
                },
                "top_k": {
                    "type": "integer",
                    "description": "Maximum number of excerpts to return (default 4)"
                }
            }),
            required: vec!["query".to_string()],
        },
    }
}

#[derive(Debug)]
pub struct FileSearchExecutor {
    db: Db,
    switchboard: SwitchboardClient,
    vllm: VllmClient,
    conversation_id: Option<String>,
    /// Request project scope, used to search project files when the conversation row doesn't exist yet (first message of a project chat).
    project_id: Option<String>,
}

impl FileSearchExecutor {
    #[must_use]
    pub const fn new(
        db: Db,
        switchboard: SwitchboardClient,
        vllm: VllmClient,
        conversation_id: Option<String>,
        project_id: Option<String>,
    ) -> Self {
        Self {
            db,
            switchboard,
            vllm,
            conversation_id,
            project_id,
        }
    }
}

#[quench_core::async_trait]
impl ToolExecutor for FileSearchExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let Some(conversation_id) = &self.conversation_id else {
            return ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: "file_search is only available within a conversation".to_string(),
                is_error: true,
            };
        };

        let Some(query) = tool_call
            .arguments
            .get("query")
            .and_then(|q| q.as_str())
            .filter(|q| !q.trim().is_empty())
        else {
            return ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: "Missing required parameter 'query'".to_string(),
                is_error: true,
            };
        };

        let top_k = tool_call
            .arguments
            .get("top_k")
            .and_then(quench_serde::Value::as_i64)
            .unwrap_or_else(|| rag::RagConfig::from_env().top_k)
            .clamp(1, 20);

        match rag::search_chunks(
            &self.db,
            &self.switchboard,
            &self.vllm,
            conversation_id,
            self.project_id.as_deref(),
            query,
            top_k,
        )
        .await
        {
            Ok(hits) if hits.is_empty() => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: "No matching content found in the uploaded files.".to_string(),
                is_error: false,
            },
            Ok(hits) => {
                let mut content = format!("Found {} relevant excerpt(s):\n\n", hits.len());
                for hit in hits {
                    let location = hit.detail.as_ref().map_or_else(
                        || format!(" · chunk {}", hit.chunk_index),
                        |d| format!(" · {d}"),
                    );
                    let _ = writeln!(
                        content,
                        "[{}{} · similarity {:.2}]\n{}\n",
                        hit.file_name, location, hit.similarity, hit.content
                    );
                }
                ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content,
                    is_error: false,
                }
            }
            Err(e) => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: format!("File search failed: {e}"),
                is_error: true,
            },
        }
    }
}
