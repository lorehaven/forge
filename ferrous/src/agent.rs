use crate::cli::render_model_progress;
use crate::llm::is_port_open;
use crate::plan::ExecutionPlan;
use crate::sessions::{load_conversation_by_prefix, save_conversation};
use crate::tools::execute_tool;
use anyhow::{Result, anyhow};
use colored::Colorize;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde_json::{Value, json};
use std::io::{self, Write};
use std::process::Child;
use std::sync::{Arc, Mutex};

static TOOLS_JSON: Lazy<Vec<Value>> = Lazy::new(|| {
    let s = include_str!("../config/tools.json");
    serde_json::from_str(s).expect("Invalid tools.json")
});

static PLAN_PROMPT: &str = r#"
You are in PLANNING MODE.

Rules:
- Do NOT call tools
- Do NOT describe human actions (editors, terminals, thinking)
- Each step MUST be directly executable using available tools
- Each step MUST start with a verb
- Prefer concrete file/tool actions

Allowed verbs (examples):
- Read file <path>
- Modify function <name> in <path>
- Replace code in <path>
- Run command <command>

Output format ONLY:
PLAN:
1. <single executable action>
2. <single executable action>
"#;

static PROMPT: &str = r#"
You are Ferrous, an expert developer and autonomous coding agent running in a project.

Your primary goal: help the user write, refactor, fix, improve, and maintain code efficiently and safely.

Core Rules:
- When the user asks to edit, refactor, fix, improve, add, remove, rename, or change ANY code/file — you MUST use write_file or replace_in_file.
  - NEVER just output a code block and say "replace this with that".
  - ALWAYS perform the actual file modification using tools.
  - Preserve all unchanged code verbatim
  - Modify only the minimal necessary lines
  - Never replace an entire file unless explicitly instructed
  - Never emit placeholders such as <updated-content> or <modified-content>
  - Always show concrete code
  - Always verify the changes by trying to build the project
- First, use read_file or list_files_recursive to understand the current code.
- For small, targeted changes → prefer replace_in_file (safer, more precise).
- For full-file rewrites or new files → use write_file.
- After any change, ALWAYS use git_diff(path) to show what was modified.
- Never use absolute paths. All paths are relative to the current working directory.
- NEVER run rm, mv, cp, git commit/push/pull/merge/rebase, curl, wget, sudo, or any destructive/network commands — they will be rejected.
- Stay inside the current project directory — no path traversal.
- Be precise, minimal, and safe. Only change exactly what is needed.
- If unsure about a file's content, read it first.
- Use search_text to quickly find code snippets, functions, or error messages across files.

You have access to these tools: read_file, write_file, replace_in_file, list_directory, get_directory_tree, create_directory, file_exists, list_files_recursive, search_text, execute_shell_command, git_status, git_diff, git_add.

Respond helpfully and concisely. Think step-by-step before calling tools.
"#;

pub struct Agent {
    client: Client,
    pub messages: Vec<Value>,
    pub server: Option<Arc<Mutex<Child>>>,
    port: u16,
}

impl Agent {
    pub fn save_conversation_named(&self, name: &str) -> Result<String> {
        let (filename, _) = save_conversation(&self.messages, name)?;
        Ok(filename)
    }

    pub fn load_conversation(&mut self, prefix: &str) -> Result<String> {
        let (conv, _) = load_conversation_by_prefix(prefix)?;
        self.messages = conv.messages;
        Ok(conv.name)
    }
}

impl Agent {
    /// Create a new Agent and spawn llama-server
    pub async fn new(
        model: &str,
        max_tokens: u32,
        temperature: f32,
        repeat_penalty: f32,
        port: u16,
        debug: bool,
    ) -> Result<Self> {
        use crate::llm::spawn_server;

        let server_handle = spawn_server(
            model,
            max_tokens,
            temperature,
            repeat_penalty,
            port,
            debug,
            Some(Box::new(render_model_progress)),
        )
        .await?;

        Ok(Self {
            client: Client::new(),
            messages: vec![json!({"role": "system", "content": PROMPT})],
            server: Some(server_handle),
            port,
        })
    }

    /// Connect to an existing server (no spawn)
    pub async fn connect_only(port: u16) -> Result<Self> {
        use crate::llm::connect_only;

        connect_only(port).await?;

        Ok(Self {
            client: Client::new(),
            messages: vec![json!({"role": "system", "content": PROMPT})],
            server: None,
            port,
        })
    }

    pub async fn generate_plan(&self, query: &str) -> Result<ExecutionPlan> {
        let mut temp_messages = self.messages.clone();
        temp_messages.push(json!({"role": "user", "content": format!("{}{}", PLAN_PROMPT, query)}));

        let body = json!({
        "messages": &temp_messages,
        "temperature": 0.1,
        "max_tokens": 1024,
        "stream": false,
    });

        let resp: Value = self
            .client
            .post(format!("http://127.0.0.1:{}/v1/chat/completions", self.port))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");

        let mut steps = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if let Some(idx) = line.find('.')
                && line[..idx].chars().all(|c| c.is_ascii_digit())
            {
                let step = line[idx + 1..].trim();
                if !step.is_empty() {
                    steps.push(step.to_string());
                }
            }
        }

        if steps.is_empty() {
            anyhow::bail!("Planner produced no steps");
        }

        Ok(ExecutionPlan::new(steps))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn stream(
        &mut self,
        user_input: &str,
        temperature: f32,
        top_p: f32,
        min_p: f32,
        top_k: i32,
        repeat_penalty: f32,
        max_tokens: u32,
        mirostat: i32,
        mirostat_tau: f32,
        mirostat_eta: f32,
    ) -> Result<String> {
        self.ensure_alive().await?;

        self.messages
            .push(json!({"role": "user", "content": user_input}));
        let tools = &*TOOLS_JSON;

        loop {
            let body = json!({
            "messages": &self.messages,
            "tools": &tools,
            "tool_choice": "auto",
            "temperature": temperature,
            "top_p": if mirostat > 0 { 1. } else { top_p },
            "min_p": min_p,
            "top_k": if mirostat > 0 { 0 } else { top_k },
            "repeat_penalty": repeat_penalty,
            "max_tokens": max_tokens,
            "stream": true,
            "mirostat": mirostat,
            "mirostat_tau": mirostat_tau,
            "mirostat_eta": mirostat_eta,
        });

            let response = self
                .client
                .post(format!("http://127.0.0.1:{}/v1/chat/completions", self.port))
                .json(&body)
                .send()
                .await?
                .error_for_status()?;

            let mut stream = response.bytes_stream();

            let mut full_content = String::new();
            let mut tool_calls: Vec<Value> = vec![];
            let mut has_started_printing = false;

            while let Some(item) = stream.next().await {
                let bytes = item?;

                // Convert bytes to string and split into lines (SSE events are line-delimited)
                let data = String::from_utf8_lossy(&bytes).to_string();
                for line in data.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if !trimmed.starts_with("data: ") {
                        continue;
                    }
                    let payload = &trimmed[6..];
                    if payload == "[DONE]" {
                        break;
                    }
                    let chunk: Value = match serde_json::from_str(payload) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let delta = &chunk["choices"][0]["delta"];

                    // Handle content delta (print immediately for streaming effect)
                    if let Some(content) = delta["content"].as_str() {
                        if !has_started_printing {
                            print!("{}", "│ ".dimmed());
                            has_started_printing = true;
                        }
                        print!("{}", content);
                        io::stdout().flush()?;
                        full_content.push_str(content);
                    }

                    // Handle tool_calls deltas
                    if let Some(tc_deltas) = delta["tool_calls"].as_array() {
                        for tc_delta in tc_deltas {
                            let index = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                            while tool_calls.len() <= index {
                                tool_calls.push(json!({
                                "type": "function",
                                "id": "",
                                "function": { "name": "", "arguments": "" }
                            }));
                            }
                            if let Some(id) = tc_delta["id"].as_str() {
                                tool_calls[index]["id"] = json!(id);
                            }
                            if let Some(name_delta) = tc_delta["function"]["name"].as_str() {
                                let current_name = tool_calls[index]["function"]["name"].as_str().unwrap_or("");
                                tool_calls[index]["function"]["name"] = json!(format!("{}{}", current_name, name_delta));
                            }
                            if let Some(args_delta) = tc_delta["function"]["arguments"].as_str() {
                                let current_args = tool_calls[index]["function"]["arguments"].as_str().unwrap_or("");
                                tool_calls[index]["function"]["arguments"] = json!(format!("{}{}", current_args, args_delta));
                            }
                        }
                    }
                }
            }

            if has_started_printing {
                println!();
            }

            // Construct the full message
            let content_json = if full_content.is_empty() { json!(null) } else { json!(full_content) };
            let tool_calls_json = if tool_calls.is_empty() { json!(null) } else { json!(tool_calls) };
            let message = json!({
            "role": "assistant",
            "content": content_json,
            "tool_calls": tool_calls_json
        });
            self.messages.push(message);

            // If tool calls exist, execute them
            if !tool_calls.is_empty() {
                for call in tool_calls {
                    let name = call["function"]["name"].as_str().unwrap_or("");
                    let args_str = call["function"]["arguments"].as_str().unwrap_or("{}");
                    let args: Value = match serde_json::from_str(args_str) {
                        Ok(v) => v,
                        Err(e) => {
                            self.messages.push(json!({
                            "role": "tool",
                            "tool_call_id": call["id"].as_str().unwrap_or(""),
                            "name": name,
                            "content": format!("Tool call failed: invalid arguments JSON: {}", e)
                        }));
                            continue;
                        }
                    };

                    let result = match execute_tool(name, args).await {
                        Ok(r) => r,
                        Err(e) => format!("Tool error: {}", e),
                    };

                    self.messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call["id"].as_str().unwrap_or(""),
                    "name": name,
                    "content": result
                }));
                }
                continue; // Continue the agent loop for next response
            } else {
                return Ok(full_content);
            }
        }
    }

    async fn ensure_alive(&self) -> Result<()> {
        if !is_port_open("127.0.0.1", self.port).await {
            return Err(anyhow!("llama-server not responding on port {}", self.port));
        }
        Ok(())
    }
}
