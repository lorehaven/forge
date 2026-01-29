use crate::cli::render_model_progress;
use crate::config::SamplingConfig;
use crate::llm::is_port_open;
use crate::plan::ExecutionPlan;
use crate::sessions::{load_conversation_by_prefix, save_conversation};
use crate::tools::execute_tool;
use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{Value, json};
use std::io::{self, Write};
use std::process::Child;
use std::sync::{Arc, LazyLock, Mutex};

static TOOLS_JSON: LazyLock<Vec<Value>> = LazyLock::new(|| {
    let s = include_str!("../config/tools.json");
    serde_json::from_str(s).expect("Invalid tools.json")
});

static PLAN_PROMPT: &str = r"
You are in PLANNING MODE.

Rules:
- Do NOT call tools
- Do NOT describe human actions (thinking, searching, typing)
- Each step MUST be directly executable using available tools
- Each step MUST start with a verb
- Prefer concrete file/tool actions

Allowed verbs (examples):
- Read file <path>
- Modify function <name> in <path>
- Replace text in <path>
- Run command <command>

Output format ONLY:
PLAN:
1. <single executable action>
2. <single executable action>
";

static PROMPT: &str = r"
You are Ferrous, an expert multi-purpose assistant and autonomous agent running in a project.

Your primary goal: help the user analyze, modify, improve, and maintain the project efficiently and safely.

Core Rules:
- When the user asks to edit, refactor, fix, improve, add, remove, rename, or change ANY file — you MUST use write_file or replace_in_file tool calls.
  - NEVER just output a code block and say 'replace this with that'.
  - NEVER just output a code block and say 'I'll use tool X to do Y'. You MUST actually call the tool.
  - NEVER write scripts or pseudocode that 'uses' tools. Instead, use the tool-calling mechanism of your LLM interface.
  - ALWAYS perform the actual file modification using tool calls.
  - If you need to make multiple changes to the same file or different files, call the necessary tools sequentially.
  - Preserve all unchanged content verbatim.
  - Modify only the minimal necessary lines.
  - Never replace an entire file unless explicitly instructed.
  - Never emit placeholders such as <updated-content> or <modified-content>.
  - ALWAYS verify the changes using appropriate verification tools or commands (e.g., build tools, linters, or 'execute_shell_command' for the project's specific language).
- Before performing any work, ALWAYS use discover_technologies to understand what technologies are used in the project.
- First, use read_file or list_files_recursive to understand the current state.
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
- Use search_text to quickly find snippets, functions, or error messages across files.
- Use find_file to find the exact path of a file if you only know its name.

You have access to these tools: discover_technologies, analyze_project, read_file, read_multiple_files, write_file, replace_in_file, list_directory, get_directory_tree, create_directory, file_exists, list_files_recursive, search_text, find_file, execute_shell_command, git_status, git_diff.
Respond helpfully and concisely. Think step-by-step before calling tools.
";

#[derive(Debug)]
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
        const PLAN_MAX_TOKENS: u32 = 1024;
        const SAFETY_BUFFER: u64 = 1024;

        let mut temp_messages = self.messages.clone();
        temp_messages.push(json!({"role": "user", "content": format!("{}{}", PLAN_PROMPT, query)}));

        // Plan phase also needs context management
        // (Planning usually doesn't need much, but let's be safe)

        let url_props = format!("http://127.0.0.1:{}/props", self.port);
        let props: Value = self.client.get(url_props).send().await?.json().await?;
        let n_ctx = props["n_ctx"].as_u64().unwrap_or(8192);

        let mut prompt_tokens = self.count_message_tokens(&temp_messages).await? as u64;
        while prompt_tokens
            > n_ctx
                .saturating_sub(u64::from(PLAN_MAX_TOKENS))
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

        // Enforce technology discovery as the first step for new conversations
        if self.messages.len() <= 1 {
            steps.push("Discover project technologies".to_string());
        }

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

    pub async fn stream(
        &mut self,
        user_input: &str,
        sampling: SamplingConfig,
        is_debug: bool,
    ) -> Result<String> {
        self.ensure_alive().await?;
        self.messages
            .push(json!({"role": "user", "content": user_input}));

        loop {
            let max_tokens = self.manage_context(&sampling, is_debug).await?;
            let body = self.build_request_body(&sampling, max_tokens);
            let url = format!("http://127.0.0.1:{}/v1/chat/completions", self.port);

            let resp = self.client.post(&url).json(&body).send().await?;
            if !resp.status().is_success() {
                return self
                    .handle_error_response(resp, &url, &body, is_debug)
                    .await;
            }

            let (full_content, tool_calls) = self.process_stream(resp).await?;

            if tool_calls.is_empty() {
                self.messages
                    .push(json!({"role": "assistant", "content": full_content}));
                println!();
                return Ok(full_content);
            }

            self.handle_tool_calls(full_content, tool_calls, is_debug)
                .await?;
        }
    }

    async fn manage_context(&mut self, sampling: &SamplingConfig, is_debug: bool) -> Result<u32> {
        const SAFETY_BUFFER: u64 = 1024;
        let context = sampling.context.unwrap_or(8192);
        let mut max_tokens = sampling.max_tokens.unwrap_or(4096);

        let context_u64 = u64::from(context);
        let mut prompt_tokens = self.count_message_tokens(&self.messages).await? as u64;

        let reserved_for_generation = u64::from(max_tokens).max(context_u64 / 4);
        let max_allowed_prompt = context_u64
            .saturating_sub(reserved_for_generation)
            .saturating_sub(SAFETY_BUFFER);

        while prompt_tokens > max_allowed_prompt && self.messages.len() > 2 {
            self.messages.remove(1);
            prompt_tokens = self.count_message_tokens(&self.messages).await? as u64;
        }

        let total_estimated = prompt_tokens + u64::from(max_tokens) + SAFETY_BUFFER;

        if total_estimated > context_u64 {
            let safe_max_tokens = context_u64
                .saturating_sub(prompt_tokens)
                .saturating_sub(SAFETY_BUFFER)
                .max(1024);
            if u64::from(max_tokens) > safe_max_tokens {
                if is_debug {
                    eprintln!(
                        "{} Context tight ({prompt_tokens} prompt tokens). Clamping max_tokens {max_tokens} → {safe_max_tokens}",
                        "ADJUSTED:".bright_yellow().bold(),
                    );
                }
                max_tokens = u32::try_from(safe_max_tokens).unwrap_or(1024);
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

        Ok(max_tokens)
    }

    fn build_request_body(&self, sampling: &SamplingConfig, max_tokens: u32) -> Value {
        json!({
            "messages": &self.messages,
            "tools": &*TOOLS_JSON,
            "tool_choice": "auto",
            "temperature": sampling.temperature.unwrap_or(0.2),
            "top_p": if sampling.mirostat.unwrap_or(0) > 0 { 1. } else { sampling.top_p.unwrap_or(0.95) },
            "min_p": sampling.min_p.unwrap_or(0.05),
            "top_k": if sampling.mirostat.unwrap_or(0) > 0 { 0 } else { sampling.top_k.unwrap_or(40) },
            "repeat_penalty": sampling.repeat_penalty.unwrap_or(1.1),
            "max_tokens": max_tokens,
            "stream": true,
            "mirostat": sampling.mirostat.unwrap_or(0),
            "mirostat_tau": sampling.mirostat_tau.unwrap_or(5.0),
            "mirostat_eta": sampling.mirostat_eta.unwrap_or(0.1),
        })
    }

    async fn handle_error_response(
        &self,
        resp: reqwest::Response,
        url: &str,
        body: &Value,
        is_debug: bool,
    ) -> Result<String> {
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

        if is_debug && let Ok(pretty) = serde_json::to_string_pretty(&body) {
            eprintln!("\n{}", "Request payload that failed:".yellow().bold());
            eprintln!("{pretty}");
        }

        eprintln!("\n{}", "Server response body:".yellow().bold());
        eprintln!("{error_body}");
        eprintln!("{}", "═".repeat(80).red().bold());
        eprintln!();

        Err(anyhow!(
            "llama-server returned {status}: {}",
            error_body.chars().take(500).collect::<String>()
        ))
    }

    async fn process_stream(&self, resp: reqwest::Response) -> Result<(String, Vec<Value>)> {
        let mut stream = resp.bytes_stream();
        let mut full_content = String::new();
        let mut tool_calls: Vec<Value> = vec![];
        let mut has_started_printing = false;

        while let Some(item) = stream.next().await {
            let bytes = item.context("Stream failure")?;
            let data = String::from_utf8_lossy(&bytes).to_string();

            for line in data.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || !trimmed.starts_with("data: ") {
                    continue;
                }

                let payload = &trimmed[6..];
                if payload == "[DONE]" {
                    break;
                }

                let chunk: Value = serde_json::from_str(payload).context("JSON parse error")?;
                let delta = &chunk["choices"][0]["delta"];

                if let Some(content) = delta["content"].as_str() {
                    if !has_started_printing {
                        print!("{}", "│ ".dimmed());
                        has_started_printing = true;
                    }
                    print!("{content}");
                    io::stdout().flush()?;
                    full_content.push_str(content);
                }

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
                                json!(format!("{current_name}{name_delta}"));
                        }
                        if let Some(args_delta) = tc_delta["function"]["arguments"].as_str() {
                            let current_args = tool_calls[index]["function"]["arguments"]
                                .as_str()
                                .unwrap_or("");
                            tool_calls[index]["function"]["arguments"] =
                                json!(format!("{current_args}{args_delta}"));
                        }
                    }
                }
            }
        }
        Ok((full_content, tool_calls))
    }

    async fn handle_tool_calls(
        &mut self,
        full_content: String,
        tool_calls: Vec<Value>,
        is_debug: bool,
    ) -> Result<()> {
        self.messages.push(json!({
            "role": "assistant",
            "content": if full_content.is_empty() { json!(null) } else { json!(full_content) },
            "tool_calls": tool_calls
        }));

        if !full_content.is_empty() {
            println!();
        }

        for tool_call in tool_calls {
            let id = tool_call["id"].as_str().unwrap_or("");
            let name = tool_call["function"]["name"].as_str().unwrap_or("");
            let args_raw = tool_call["function"]["arguments"].as_str().unwrap_or("{}");

            let args_parsed: Value = serde_json::from_str(args_raw).unwrap_or_else(|_| json!({}));

            println!(
                "{} {}{}",
                "  󱓞 Tool Call:".bright_yellow().bold(),
                name.bright_white().bold(),
                format!("({args_raw})").dimmed()
            );

            let tool_result = execute_tool(name, args_parsed).await;

            let result_str = match tool_result {
                Ok(r) => r,
                Err(e) => {
                    let err_msg = format!("Tool error: {e}");
                    if is_debug {
                        eprintln!("{} {err_msg}", "DEBUG:".red().bold());
                    }
                    err_msg
                }
            };

            if is_debug {
                println!(
                    "{} {}",
                    "  󱓞 Result:".bright_green().bold(),
                    result_str.dimmed()
                );
            }

            self.messages.push(json!({
                "role": "tool",
                "tool_call_id": id,
                "content": result_str
            }));
        }
        Ok(())
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
            .map(std::vec::Vec::len)
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
