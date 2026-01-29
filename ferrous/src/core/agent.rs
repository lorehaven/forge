use crate::config::{SamplingConfig, PromptManager, get_default_context};
use crate::core::ExecutionPlan;
use crate::core::sessions::{load_conversation_by_prefix, save_conversation};
use crate::llm::{StopCondition, get_stop_words_for_language, is_port_open};
use crate::tools::execute_tool;
use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{Value, json};
use std::process::Child;
use std::sync::{Arc, LazyLock, Mutex};

static TOOLS_JSON: LazyLock<Vec<Value>> = LazyLock::new(|| {
    let s = include_str!("../../config/tools.json");
    serde_json::from_str(s).expect("Invalid tools.json")
});

pub const DEFAULT_PLAN_PROMPT: &str = r"
You are in PLANNING MODE.

Rules:
- Do NOT call tools
- Do NOT describe human actions (thinking, searching, typing)
- Each step MUST be directly executable using available tools
- Each step MUST start with a verb
- Prefer concrete file/tool actions
- If the request is a general question NOT related to the project, do NOT generate a plan with tool calls. Instead, provide a single step: '1. Answer the user's question directly: <original_question>'

Allowed verbs (examples):
- Read file <path>
- Modify function <name> in <path>
- Replace text in <path>
- Run command <command>
- Answer the user's question directly: <original_question>

Output format MUST start with 'PLAN:' followed by numbered steps:
PLAN:
1. <action>
2. <action>
";

pub const DEFAULT_PROMPT: &str = r"
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
- If you need to output a code block that ITSELF contains code blocks (e.g., when showing a README.md or a Markdown file), you MUST use 4 backticks (````) for the outer block to avoid breaking the UI.
- For project-related tasks, first use read_file or list_files_recursive to understand the current state.
- For general knowledge questions unrelated to the project, do NOT use project exploration tools. Simply answer the question.
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

You have access to these tools: analyze_project, read_file, read_multiple_files, write_file, replace_in_file, list_directory, get_directory_tree, create_directory, file_exists, list_files_recursive, search_text, find_file, execute_shell_command, git_status, git_diff.
Respond helpfully and concisely. Think step-by-step before calling tools.
";

#[derive(Debug)]
pub struct Agent {
    client: Client,
    pub messages: Vec<Value>,
    pub server: Option<Arc<Mutex<Child>>>,
    port: u16,
    pub prompt_manager: PromptManager,
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
        use crate::llm::{spawn_server};

        let server_handle = spawn_server(
            model,
            context,
            temperature,
            repeat_penalty,
            port,
            debug,
            None,
        )
        .await?;

        let prompt_manager = PromptManager::new()?;
        let ctx = get_default_context();
        let system_prompt = prompt_manager.render_system(&ctx)?;

        Ok(Self {
            client: Client::new(),
            messages: vec![json!({"role": "system", "content": system_prompt})],
            server: Some(server_handle),
            port,
            prompt_manager,
        })
    }

    /// Connect to an existing server (no spawn)
    pub async fn connect_only(port: u16) -> Result<Self> {
        use crate::llm::{connect_only};

        connect_only(port).await?;

        let prompt_manager = PromptManager::new()?;
        let ctx = get_default_context();
        let system_prompt = prompt_manager.render_system(&ctx)?;

        Ok(Self {
            client: Client::new(),
            messages: vec![json!({"role": "system", "content": system_prompt})],
            server: None,
            port,
            prompt_manager,
        })
    }

    pub async fn generate_plan(&self, query: &str) -> Result<ExecutionPlan> {
        const PLAN_MAX_TOKENS: u32 = 1024;
        const SAFETY_BUFFER: u64 = 1024;

        let mut temp_messages = self.messages.clone();
        let ctx = get_default_context();
        let planner_prompt = self.prompt_manager.render_planner(&ctx)?;
        temp_messages
            .push(json!({"role": "user", "content": format!("{}{}", planner_prompt, query)}));

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
            if content
                .to_lowercase()
                .contains("answer the user's question directly")
            {
                steps.push(format!("Answer the user's question directly: {query}"));
            } else if !content.trim().is_empty() {
                // If it's not the specific instruction but also not empty and not numbered,
                // just take the whole thing as a single step if it looks like a step
                let fallback = content.trim().lines().next().unwrap_or("").trim();
                if !fallback.is_empty() && fallback.len() < 100 {
                    steps.push(fallback.to_string());
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
        interaction: &dyn crate::ui::interface::InteractionHandler,
    ) -> Result<String> {
        self.ensure_alive().await?;
        self.messages
            .push(json!({"role": "user", "content": user_input}));

        loop {
            let max_tokens = self
                .manage_context(&sampling, is_debug, interaction)
                .await?;
            let body = self.build_request_body(&sampling, max_tokens);
            let url = format!("http://127.0.0.1:{}/v1/chat/completions", self.port);

            let resp = self.client.post(&url).json(&body).send().await?;
            if !resp.status().is_success() {
                return self
                    .handle_error_response(resp, &url, &body, is_debug, interaction)
                    .await;
            }

            let (full_content, tool_calls) = self.process_stream(resp, interaction).await?;

            if tool_calls.is_empty() {
                self.messages
                    .push(json!({"role": "assistant", "content": full_content}));
                // interaction.print_stream_end() already handled the newline for streaming text
                return Ok(full_content);
            }

            self.handle_tool_calls(full_content, tool_calls, is_debug, interaction)
                .await?;
        }
    }

    async fn manage_context(
        &mut self,
        sampling: &SamplingConfig,
        is_debug: bool,
        interaction: &dyn crate::ui::interface::InteractionHandler,
    ) -> Result<u32> {
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
                    interaction.print_debug(&format!(
                        "Context tight ({prompt_tokens} prompt tokens). Clamping max_tokens {max_tokens} → {safe_max_tokens}",
                    ));
                }
                max_tokens = u32::try_from(safe_max_tokens).unwrap_or(1024);
            }
        }

        if is_debug {
            let used_percent = (total_estimated * 100 / context_u64).min(100);
            interaction.print_debug(&format!(
                "Context usage: ~{total_estimated}/{context_u64} tokens ({used_percent}%). Messages: {}",
                self.messages.len()
            ));
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
        interaction: &dyn crate::ui::interface::InteractionHandler,
    ) -> Result<String> {
        let status = resp.status();
        let error_body = resp
            .text()
            .await
            .unwrap_or_else(|_| "(no response body)".to_string());

        interaction.print_error(&format!("LLM server returned non-success status: {status}"));
        interaction.print_info(&format!("URL: {url}"));

        if is_debug && let Ok(pretty) = serde_json::to_string_pretty(&body) {
            interaction.print_debug("Request payload that failed:");
            interaction.print_debug(&pretty);
        }

        interaction.print_error("Server response body:");
        interaction.print_error(&error_body);

        Err(anyhow!(
            "llama-server returned {status}: {}",
            error_body.chars().take(500).collect::<String>()
        ))
    }

    async fn process_stream(
        &self,
        resp: reqwest::Response,
        interaction: &dyn crate::ui::interface::InteractionHandler,
    ) -> Result<(String, Vec<Value>)> {
        let mut stream = resp.bytes_stream();
        let mut full_content = String::new();
        let mut tool_calls: Vec<Value> = vec![];
        let mut has_started_printing = false;
        let mut has_started_tool_printing = false;
        let mut buffer = String::new();
        let mut in_code_block = false;
        let mut code_fence_buffer = String::new();
        let mut current_fence_size = 0;
        let mut potential_lang = String::new();
        let mut waiting_for_lang = false;

        // Initialize stop condition
        // For now, we use a generic set of stop words. 
        // In the future, we could detect language from context.
        let stop_words = get_stop_words_for_language("unknown");
        let mut stop_condition = StopCondition::new(stop_words);

        while let Some(item) = stream.next().await {
            let bytes = item.context("Stream failure")?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].to_string();
                buffer = buffer[pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                let payload = line
                    .strip_prefix("data: ")
                    .or_else(|| line.trim_start().strip_prefix("data: "))
                    .unwrap_or(&line);

                let chunk: Value = if let Ok(v) = serde_json::from_str(payload) {
                    v
                } else {
                    let payload_trimmed = payload.trim();
                    if payload_trimmed.is_empty() {
                        continue;
                    }
                    if payload_trimmed == "[DONE]" {
                        break;
                    }
                    match serde_json::from_str(payload_trimmed) {
                        Ok(v) => v,
                        Err(_) => continue,
                    }
                };
                let delta = &chunk["choices"][0]["delta"];

                if let Some(content) = delta["content"].as_str() {
                    if !has_started_printing {
                        interaction.print_stream_start();
                        has_started_printing = true;
                    }

                    let (should_stop, _match_len) = stop_condition.should_stop(content);
                    if should_stop {
                        // Abort stream processing
                        if has_started_printing {
                            interaction.print_stream_end();
                        }
                        return Ok((full_content, tool_calls));
                    }

                    Self::process_content_delta(
                        content,
                        interaction,
                        &mut in_code_block,
                        &mut code_fence_buffer,
                        &mut current_fence_size,
                        &mut potential_lang,
                        &mut waiting_for_lang,
                    );
                    full_content.push_str(content);
                }

                if let Some(tc_deltas) = delta["tool_calls"].as_array() {
                    if !has_started_tool_printing {
                        interaction.print_stream_tool_start();
                        has_started_tool_printing = true;
                    }
                    Self::process_tool_calls_delta(tc_deltas, &mut tool_calls, interaction);
                }
            }
        }
        if has_started_printing {
            interaction.print_stream_end();
        }
        if has_started_tool_printing {
            interaction.print_stream_tool_end();
        }
        Ok((full_content, tool_calls))
    }

    #[allow(clippy::too_many_arguments)]
    fn process_content_delta(
        content: &str,
        interaction: &dyn crate::ui::interface::InteractionHandler,
        in_code_block: &mut bool,
        code_fence_buffer: &mut String,
        current_fence_size: &mut usize,
        potential_lang: &mut String,
        waiting_for_lang: &mut bool,
    ) {
        for c in content.chars() {
            if *waiting_for_lang {
                if c == '\n' {
                    interaction.print_stream_code_start(potential_lang);
                    potential_lang.clear();
                    *waiting_for_lang = false;
                } else {
                    potential_lang.push(c);
                }
                continue;
            }

            if c == '`' {
                code_fence_buffer.push(c);
            } else {
                if !code_fence_buffer.is_empty() {
                    let fence_len = code_fence_buffer.len();
                    if fence_len >= 3 {
                        if *in_code_block {
                            if fence_len == *current_fence_size {
                                interaction.print_stream_code_end();
                                *in_code_block = false;
                                *current_fence_size = 0;
                            } else {
                                // Nested fence of different size, just print it
                                interaction.print_stream_code_chunk(code_fence_buffer);
                            }
                        } else {
                            *in_code_block = true;
                            *current_fence_size = fence_len;
                            *waiting_for_lang = true;
                        }
                    } else {
                        // Not enough backticks for a fence
                        if *in_code_block {
                            interaction.print_stream_code_chunk(code_fence_buffer);
                        } else {
                            interaction.print_stream_chunk(code_fence_buffer);
                        }
                    }
                    code_fence_buffer.clear();
                }

                if *waiting_for_lang {
                    if c == '\n' {
                        interaction.print_stream_code_start("");
                        *waiting_for_lang = false;
                    } else {
                        potential_lang.push(c);
                    }
                } else if c == '\n' && *in_code_block {
                    interaction.print_stream_code_chunk("\n");
                } else if *in_code_block {
                    interaction.print_stream_code_chunk(&c.to_string());
                } else {
                    interaction.print_stream_chunk(&c.to_string());
                }
            }
        }
    }

    fn process_tool_calls_delta(
        tc_deltas: &[Value],
        tool_calls: &mut Vec<Value>,
        interaction: &dyn crate::ui::interface::InteractionHandler,
    ) {
        for tc_delta in tc_deltas {
            let index = usize::try_from(tc_delta["index"].as_u64().unwrap_or(0)).unwrap();
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
                interaction.print_stream_tool_chunk(name_delta);
                let current_name = tool_calls[index]["function"]["name"].as_str().unwrap_or("");
                tool_calls[index]["function"]["name"] =
                    json!(format!("{current_name}{name_delta}"));
            }
            if let Some(args_delta) = tc_delta["function"]["arguments"].as_str() {
                interaction.print_stream_tool_chunk(args_delta);
                let current_args = tool_calls[index]["function"]["arguments"]
                    .as_str()
                    .unwrap_or("");
                tool_calls[index]["function"]["arguments"] =
                    json!(format!("{current_args}{args_delta}"));
            }
        }
    }

    async fn handle_tool_calls(
        &mut self,
        full_content: String,
        tool_calls: Vec<Value>,
        is_debug: bool,
        interaction: &dyn crate::ui::interface::InteractionHandler,
    ) -> Result<()> {
        self.messages.push(json!({
            "role": "assistant",
            "content": if full_content.is_empty() { json!(null) } else { json!(full_content) },
            "tool_calls": tool_calls
        }));

        for tool_call in tool_calls {
            let id = tool_call["id"].as_str().unwrap_or("");
            let name = tool_call["function"]["name"].as_str().unwrap_or("");
            let args_raw = tool_call["function"]["arguments"].as_str().unwrap_or("{}");

            let args_parsed: Value = serde_json::from_str(args_raw).unwrap_or_else(|_| json!({}));

            let tool_result = execute_tool(name, args_parsed).await;

            let result_str = match tool_result {
                Ok(r) => r,
                Err(e) => {
                    let err_msg = format!("Tool error: {e}");
                    interaction.print_debug(&err_msg);
                    err_msg
                }
            };

            if is_debug {
                interaction.print_debug(&format!("Tool Result: {result_str}"));
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
