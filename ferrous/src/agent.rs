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

static PLAN_PROMPT: &str = r"
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
";

static PROMPT: &str = r"
You are Ferrous, an expert developer and autonomous coding agent running in a project.

Your primary goal: help the user write, refactor, fix, improve, and maintain code efficiently and safely.

Core Rules:
- When the user asks to edit, refactor, fix, improve, add, remove, rename, or change ANY code/file — you MUST use write_file or replace_in_file tool calls.
  - NEVER just output a code block and say 'replace this with that'.
  - NEVER just output a code block and say 'I'll use tool X to do Y'. You MUST actually call the tool.
  - NEVER write code, shell scripts, or pseudocode that 'uses' tools (e.g., `write_file(path, content).unwrap()`). Instead, use the tool-calling mechanism of your LLM interface.
  - ALWAYS perform the actual file modification using tool calls.
  - If you need to make multiple changes to the same file or different files, call the necessary tools sequentially.
  - Preserve all unchanged code verbatim.
  - Modify only the minimal necessary lines.
  - Never replace an entire file unless explicitly instructed.
  - Never emit placeholders such as <updated-content> or <modified-content>.
  - Always show concrete code.
  - ALWAYS verify the changes by trying to build the project using execute_shell_command('cargo check').
- First, use read_file or list_files_recursive to understand the current code.
- For small, targeted changes → prefer replace_in_file.
  - IMPORTANT: replace_in_file performs EXACT string matching. You MUST read the file first and copy the text EXACTLY as it appears, including ALL whitespace, indentation, and newlines.
  - If a replacement fails (returns 'No changes made...'), it means your 'search' string did not match the file content exactly. You MUST read the file again to get the exact content.
- For full-file rewrites or new files → use write_file.
- After any change, ALWAYS use git_diff(path) to show what was modified.
- Never use absolute paths. All paths are relative to the current working directory.
- NEVER run rm, mv, cp, git commit/push/pull/merge/rebase, curl, wget, sudo, or any destructive/network commands — they will be rejected.
- Stay inside the current project directory — no path traversal.
- Be precise, minimal, and safe. Only change exactly what is needed.
- If unsure about a file's content, read it first.
- Use search_text to quickly find code snippets, functions, or error messages across files.

You have access to these tools: analyze_project, read_file, read_multiple_files, write_file, replace_in_file, list_directory, get_directory_tree, create_directory, file_exists, list_files_recursive, search_text, execute_shell_command, git_status, git_diff.
Respond helpfully and concisely. Think step-by-step before calling tools.
";

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
        context: u32,
        temperature: f32,
        repeat_penalty: f32,
        port: u16,
        debug: bool,
    ) -> Result<Self> {
        use crate::llm::spawn_server;

        let server_handle = spawn_server(
            model,
            context,
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

        // Plan phase also needs context management
        // (Planning usually doesn't need much, but let's be safe)
        const PLAN_MAX_TOKENS: u32 = 1024;
        const SAFETY_BUFFER: u64 = 1024;

        let url_props = format!("http://127.0.0.1:{}/props", self.port);
        let props: Value = self.client.get(url_props).send().await?.json().await?;
        let n_ctx = props["n_ctx"].as_u64().unwrap_or(8192);

        let mut prompt_tokens = self.count_message_tokens(&temp_messages).await? as u64;
        while prompt_tokens
            > n_ctx
                .saturating_sub(PLAN_MAX_TOKENS as u64)
                .saturating_sub(SAFETY_BUFFER)
            && temp_messages.len() > 2
        {
            temp_messages.remove(1);
            prompt_tokens = self.count_message_tokens(&temp_messages).await? as u64;
        }

        let body = json!({
            "messages": &temp_messages,
            "temperature": 0.1,
            "max_tokens": PLAN_MAX_TOKENS,
            "stream": false,
        });

        let resp: Value = self
            .client
            .post(format!(
                "http://127.0.0.1:{}/v1/chat/completions",
                self.port
            ))
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
        context: u32,
        mut max_tokens: u32,
        mirostat: i32,
        mirostat_tau: f32,
        mirostat_eta: f32,
        is_debug: bool,
    ) -> Result<String> {
        self.ensure_alive().await?;

        self.messages
            .push(json!({"role": "user", "content": user_input}));

        let tools = &*TOOLS_JSON;

        loop {
            // ── Context Management ──────────────────────────────────────────
            // We need to fit: messages + tools + max_tokens + safety buffer
            let context_u64 = context as u64;

            // 1. Estimate tokens accurately
            let mut prompt_tokens = self.count_message_tokens(&self.messages).await? as u64;

            // 2. Sliding window if we exceed context
            // We want to keep at least 25% of the context for generation (max_tokens)
            // and some buffer for tools.
            let reserved_for_generation = (max_tokens as u64).max(context_u64 / 4);
            let safety_buffer = 1024;
            let max_allowed_prompt = context_u64
                .saturating_sub(reserved_for_generation)
                .saturating_sub(safety_buffer);

            while prompt_tokens > max_allowed_prompt && self.messages.len() > 2 {
                // Remove the oldest non-system message
                self.messages.remove(1);
                prompt_tokens = self.count_message_tokens(&self.messages).await? as u64;
            }

            // 3. Final clamping of max_tokens if still tight
            let total_estimated = prompt_tokens + max_tokens as u64 + safety_buffer;

            if total_estimated > context_u64 {
                let safe_max_tokens = context_u64
                    .saturating_sub(prompt_tokens)
                    .saturating_sub(safety_buffer)
                    .max(1024);
                if (max_tokens as u64) > safe_max_tokens {
                    if is_debug {
                        eprintln!(
                            "{} Context tight ({prompt_tokens} prompt tokens). Clamping max_tokens {max_tokens} → {safe_max_tokens}",
                            "ADJUSTED:".bright_yellow().bold(),
                        );
                    }
                    max_tokens = safe_max_tokens as u32;
                }
            }

            if is_debug {
                let used_percent = (total_estimated * 100 / context_u64).min(100);
                eprintln!(
                    "{} Context usage: ~{total_estimated}/{context_u64} tokens ({used_percent}%). Messages: {}",
                    "INFO:".cyan().bold(),
                    self.messages.len()
                );
            }

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

            let url = format!("http://127.0.0.1:{}/v1/chat/completions", self.port);

            let response = self.client.post(&url).json(&body).send().await;

            let resp = match response {
                Ok(r) => r,
                Err(e) => {
                    eprintln!(
                        "{} Network error contacting llama-server: {e}",
                        "ERROR:".red().bold()
                    );
                    return Err(anyhow!("Failed to reach llama-server: {e}"));
                }
            };

            // ── Handle non-successful HTTP responses ──
            if !resp.status().is_success() {
                let status = resp.status();
                let error_body = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "(no response body)".to_string());

                eprintln!("\n{}", "═".repeat(80).red().bold());
                eprintln!(
                    "{} LLM server returned non-success status",
                    "CRITICAL ERROR:".red().bold()
                );
                eprintln!("URL: {}", url.bright_white());
                eprintln!(
                    "Status: {} {}",
                    status.as_u16().to_string().red().bold(),
                    status.canonical_reason().unwrap_or("").bright_red()
                );

                if is_debug {
                    // Pretty-print the request payload we sent
                    if let Ok(pretty) = serde_json::to_string_pretty(&body) {
                        eprintln!("\n{}", "Request payload that failed:".yellow().bold());
                        eprintln!("{pretty}");
                    } else {
                        eprintln!("\n{}", "Could not format request body".yellow().bold());
                        eprintln!("Raw body: {body:?}");
                    }
                }

                eprintln!("\n{}", "Server response body:".yellow().bold());
                eprintln!("{}", error_body);

                eprintln!("{}", "═".repeat(80).red().bold());
                eprintln!();

                return Err(anyhow!(
                    "llama-server returned {}: {}",
                    status,
                    error_body.chars().take(500).collect::<String>()
                ));
            }

            let mut stream = resp.bytes_stream();

            let mut full_content = String::new();
            let mut tool_calls: Vec<Value> = vec![];
            let mut has_started_printing = false;

            while let Some(item) = stream.next().await {
                let bytes = match item {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("{} Stream read error: {}", "ERROR:".red().bold(), e);
                        return Err(anyhow!("Stream failure: {e}"));
                    }
                };

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
                        Err(e) => {
                            eprintln!(
                                "{} JSON parse error in stream chunk: {}",
                                "WARN:".yellow().bold(),
                                e
                            );
                            eprintln!("Bad chunk: {}", payload);
                            continue;
                        }
                    };
                    let delta = &chunk["choices"][0]["delta"];

                    // Handle content delta (print immediately for streaming effect)
                    if let Some(content) = delta["content"].as_str() {
                        if !has_started_printing {
                            print!("{}", "│ ".dimmed());
                            has_started_printing = true;
                        }
                        print!("{content}");
                        io::stdout().flush()?;
                        full_content.push_str(content);
                    }

                    // Handle tool_calls deltas
                    if let Some(tc_deltas) = delta["tool_calls"].as_array() {
                        for tc_delta in tc_deltas {
                            let index =
                                usize::try_from(tc_delta["index"].as_u64().unwrap_or(0)).unwrap();
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
                                let current_name =
                                    tool_calls[index]["function"]["name"].as_str().unwrap_or("");
                                tool_calls[index]["function"]["name"] =
                                    json!(format!("{}{}", current_name, name_delta));
                            }
                            if let Some(args_delta) = tc_delta["function"]["arguments"].as_str() {
                                let current_args = tool_calls[index]["function"]["arguments"]
                                    .as_str()
                                    .unwrap_or("");
                                tool_calls[index]["function"]["arguments"] =
                                    json!(format!("{}{}", current_args, args_delta));
                            }
                        }
                    }
                }
            }

            if has_started_printing {
                println!();
            }

            // Construct the full message
            let content_json = if full_content.is_empty() {
                json!(null)
            } else {
                json!(full_content)
            };
            let tool_calls_json = if tool_calls.is_empty() {
                json!(null)
            } else {
                json!(tool_calls)
            };
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

                    let result = execute_tool(name, args)
                        .await
                        .unwrap_or_else(|e| format!("Tool error: {}", e));

                    self.messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call["id"].as_str().unwrap_or(""),
                        "name": name,
                        "content": result
                    }));
                }
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

    async fn count_tokens(&self, content: &str) -> Result<usize> {
        let url = format!("http://127.0.0.1:{}/tokenize", self.port);
        let resp: Value = self
            .client
            .post(url)
            .json(&json!({ "content": content }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        resp["tokens"]
            .as_array()
            .map(|a| a.len())
            .ok_or_else(|| anyhow!("Invalid response from /tokenize"))
    }

    async fn count_message_tokens(&self, messages: &[Value]) -> Result<usize> {
        // Approximate overhead for chat format (im_start, role, im_end, etc.)
        // For Qwen-style: <|im_start|>role\ncontent<|im_end|>\n
        let mut total = 0;
        for msg in messages {
            let role = msg["role"].as_str().unwrap_or("");
            let content = msg["content"].as_str().unwrap_or("");

            // Handle content being a string or potentially null (for assistant tool calls without content)
            if !content.is_empty() {
                total += self.count_tokens(content).await?;
            }

            total += self.count_tokens(role).await?;

            // If it's a tool call, we should also account for the tool calls and results
            if let Some(tool_calls) = msg["tool_calls"].as_array() {
                for call in tool_calls {
                    let call_str = call.to_string();
                    total += self.count_tokens(&call_str).await?;
                }
            }

            total += 8; // Constant overhead per message
        }
        Ok(total)
    }
}
