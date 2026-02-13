use crate::config::{
    Config, ModelBackend, ModelRole, PromptManager, SamplingConfig, get_default_context,
};
use crate::core::sessions::{load_conversation_by_prefix, save_conversation};
use crate::core::{ExecutionPlan, Indexer};
use crate::llm::{ModelManager, is_port_open};
use crate::tools::execute_tool;
use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{Value, json};
use std::sync::LazyLock;

static TOOLS_JSON: LazyLock<Vec<Value>> = LazyLock::new(|| {
    let s = include_str!("../../config/tools.json");
    serde_json::from_str(s).expect("Invalid tools.json")
});

pub const DEFAULT_PLAN_PROMPT: &str = r"
Generate a simple plan with numbered steps. Each step describes a high-level action.

CRITICAL: Do NOT write function calls, tool names, backticks, parentheses, or code. Write in plain English only.
IMPORTANT: Avoid vague phrases like 'as needed' or 'if necessary'. Be specific about what should be done.

Output format:
PLAN:
1. <action>
2. <action>
";

pub const DEFAULT_PROMPT: &str = r"
You are Ferrous, an expert multi-purpose assistant and autonomous agent running in a project.

Your primary goal: help the user analyze, modify, improve, and maintain the project efficiently and safely.

IMPORTANT: All file paths are relative to the current working directory where ferrous was invoked. When the user mentions a file, use that EXACT path. The current directory is the workspace root.

Core Rules:
- When the user asks to edit, refactor, fix, improve, add, remove, rename, or change ANY file — you MUST use write_file or replace_in_file tool calls.
  - NEVER just output a code block and say 'replace this with that'.
  - NEVER just output a code block and say 'I'll use tool X to do Y'. You MUST actually call the tool.
  - NEVER write scripts or pseudocode that 'uses' tools. Instead, use the tool-calling mechanism of your LLM interface.
  - ALWAYS perform the actual file modification using tool calls.
  - If you need to make multiple changes to the same file or different files, call the necessary tools sequentially.
- IMPORTANT: Qwen2.5-Coder model, when using tools, YOU MUST use the specific function call format in your delta response.
- If you decide to call a tool, you MUST NOT output anything in the 'content' field. Instead, use 'tool_calls'.
- Preserve all unchanged content verbatim.
  - Modify only the minimal necessary lines.
  - Never replace an entire file unless explicitly instructed.
  - Never emit placeholders such as <updated-content> or <modified-content>.
  - ALWAYS verify the changes using appropriate verification tools or commands (e.g., build tools, linters, or 'execute_shell_command' for cargo/rustfmt commands).
- If you need to output a code block that ITSELF contains code blocks (e.g., when showing a README.md or a Markdown file), you MUST use 4 backticks (````) for the outer block to avoid breaking the UI.
- For project-related tasks, first use read_file or list_files_recursive to understand the current state.
- For general knowledge questions unrelated to the project, do NOT use project exploration tools. Simply answer the question.
- IMPORTANT: When you are in the EXECUTION phase (following a plan), you MUST use tool calls to perform any project manipulation (like creating files, writing content, or running commands).
  - DO NOT just describe the tool you would use.
  - DO NOT just say 'I will use tool X'.
  - You MUST actually invoke the tool using the tool-calling mechanism.
  - If a step in the plan says 'create file X', you MUST call 'write_file'.
  - If a step says 'run command Y', you MUST call 'execute_shell_command'.
- For small, targeted changes → prefer replace_in_file.
  - IMPORTANT: replace_in_file performs EXACT string matching and includes pre-flight validation. You MUST read the file first and copy the text EXACTLY as it appears, including ALL whitespace, indentation, and newlines.
  - If validation fails with 'Search string not found', it means your 'search' string did not match the file content exactly. You MUST read the file again to get the exact content.
- For full-file rewrites or new files → use write_file.
- After any change, consider using lint_file(path) to verify code integrity, then use git_diff(path) to show what was modified.
- Never use absolute paths. All paths are relative to the current working directory.
- NEVER run rm, mv, cp, git commit/push/pull/merge/rebase, curl, wget, sudo, or any destructive/network commands — they will be rejected.
- Stay inside the current project directory — no path traversal.
- Be precise, minimal, and safe. Only change exactly what is needed.
- If unsure about a file's content, read it first.
- Use search_text to quickly find snippets, functions, or error messages across files.
- Use find_file to find the exact path of a file if you only know its name.

You have access to these tools: analyze_project, read_file, read_multiple_files, write_file, replace_in_file, list_directory, get_directory_tree, create_directory, file_exists, list_files_recursive, search_text, find_file, search_code_semantic, lint_file, review_code, review_module, suggest_refactorings, execute_shell_command, git_status, git_diff.
Respond helpfully and concisely. Think step-by-step before calling tools.
";

#[derive(Debug)]
pub struct Agent {
    client: Client,
    pub messages: Vec<Value>,
    pub model_manager: ModelManager,
    pub prompt_manager: PromptManager,
    pub indexer: Option<Indexer>,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub struct StreamOutcome {
    pub response: String,
    pub tool_calls_executed: usize,
}

fn parse_indexed_step(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut chars = trimmed.char_indices();
    let mut end_digits = None;
    for (idx, ch) in chars.by_ref() {
        if ch.is_ascii_digit() {
            end_digits = Some(idx + ch.len_utf8());
        } else {
            break;
        }
    }

    let end_digits = end_digits?;

    let rest = &trimmed[end_digits..];
    let mut rest_chars = rest.chars();
    let marker = rest_chars.next()?;
    if !matches!(marker, '.' | ')' | ':') {
        return None;
    }

    let step = rest_chars.as_str().trim();
    if step.is_empty() {
        None
    } else {
        Some(step.to_string())
    }
}

pub fn parse_plan_steps(content: &str, query: &str) -> Vec<String> {
    let mut steps = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let lower = line.to_lowercase();
        if lower == "plan:" || lower.starts_with("plan ") || line == "```" {
            continue;
        }

        if let Some(step) = parse_indexed_step(line) {
            steps.push(step);
            continue;
        }

        let bullet_step = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "));
        if let Some(step) = bullet_step {
            let step = step.trim();
            if !step.is_empty() {
                steps.push(step.to_string());
            }
        }
    }

    if steps.is_empty() {
        let lower = content.to_lowercase();
        if lower.contains("answer the user's question directly") {
            steps.push(format!("Answer the user's question directly: {query}"));
            return steps;
        }

        let fallback = content
            .lines()
            .map(str::trim)
            .find(|line| {
                !line.is_empty()
                    && *line != "```"
                    && !line.eq_ignore_ascii_case("plan:")
                    && !line.to_lowercase().starts_with("plan ")
            })
            .unwrap_or("");

        if !fallback.is_empty() && fallback.len() <= 240 {
            steps.push(fallback.to_string());
        }
    }

    steps
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
    /// Create a new Agent with the given config
    pub fn with_config(config: Config) -> Result<Self> {
        let model_manager = ModelManager::new();

        // Check if we have models defined, otherwise we might need some defaults or error out
        if config.models.is_empty() {
            return Err(anyhow!(
                "No models configured. Please check your .ferrous/config.toml"
            ));
        }

        let prompt_manager = PromptManager::new()?;
        let ctx = get_default_context();
        let system_prompt = prompt_manager.render_system(&ctx)?;

        let index_path = std::env::current_dir()?.join(".ferrous").join("index");
        let indexer = Indexer::new(&index_path).ok();
        if let Some(ref idx) = indexer {
            let _ = idx.index_project(&std::env::current_dir()?);
        }

        Ok(Self {
            client: Client::new(),
            messages: vec![json!({"role": "system", "content": system_prompt})],
            model_manager,
            prompt_manager,
            indexer,
            config,
        })
    }

    /// Legacy constructor (for backward compatibility or simpler use cases)
    pub fn new(
        model: &str,
        context: u32,
        temperature: f32,
        repeat_penalty: f32,
        port: u16,
        debug: bool,
    ) -> Result<Self> {
        use crate::config::ModelBackend;

        let mut config = Config::default();
        config.models.insert(
            ModelRole::Chat,
            ModelBackend::LocalLlama {
                model_path: model.to_string(),
                port,
                context_size: context,
                num_gpu_layers: 999,
            },
        );
        config.sampling.temperature = Some(temperature);
        config.sampling.repeat_penalty = Some(repeat_penalty);
        config.sampling.context = Some(context);
        config.debug = Some(debug);

        Self::with_config(config)
    }

    /// Connect to an existing server (no spawn)
    pub async fn connect_only(port: u16) -> Result<Self> {
        use crate::config::ModelBackend;
        use crate::llm::connect_only;

        connect_only(port).await?;

        let mut config = Config::default();
        config.models.insert(
            ModelRole::Chat,
            ModelBackend::LocalLlama {
                model_path: "existing-server".to_string(),
                port,
                context_size: 8192,
                num_gpu_layers: 999,
            },
        );

        Self::with_config(config)
    }

    pub async fn get_model_url(&mut self, role: ModelRole) -> Result<String> {
        let mut backend = self
            .config
            .models
            .get(&role)
            .ok_or_else(|| anyhow!("Model for role {role:?} not configured"))?
            .clone();

        // Resolve path with base_model_path if applicable
        if let ModelBackend::LocalLlama {
            ref mut model_path, ..
        } = backend
            && !model_path.starts_with('/')
            && !model_path.starts_with('.')
            && let Some(ref base) = self.config.base_model_path
        {
            let base_path = std::path::Path::new(base);
            *model_path = base_path.join(&model_path).to_string_lossy().to_string();
        }

        let handle = self
            .model_manager
            .get_or_start_model(
                role,
                &backend,
                &self.config.sampling,
                self.config.debug.unwrap_or(false),
            )
            .await?;

        match handle.backend {
            ModelBackend::LocalLlama { port, .. } => Ok(format!("http://127.0.0.1:{port}")),
            ModelBackend::OpenAi { api_base, .. } => {
                Ok(api_base.unwrap_or_else(|| "https://api.openai.com/v1".to_string()))
            }
            ModelBackend::Anthropic { .. } => Ok("https://api.anthropic.com/v1".to_string()),
            ModelBackend::External { api_base, .. } => Ok(api_base),
        }
    }

    async fn get_runtime_context_limit(&mut self, role: ModelRole) -> u64 {
        let Ok(base_url) = self.get_model_url(role).await else {
            return 8192;
        };
        let url_props = format!("{base_url}/props");

        let Ok(resp) = self.client.get(url_props).send().await else {
            return 8192;
        };
        let Ok(props) = resp.json::<Value>().await else {
            return 8192;
        };

        props["n_ctx"].as_u64().unwrap_or(8192)
    }

    pub async fn generate_plan(&mut self, query: &str) -> Result<ExecutionPlan> {
        const PLAN_MAX_TOKENS: u32 = 1024;
        const SAFETY_BUFFER: u64 = 1024;

        let mut temp_messages = self.messages.clone();
        let ctx = get_default_context();
        let planner_prompt = self.prompt_manager.render_planner(&ctx)?;
        temp_messages
            .push(json!({"role": "user", "content": format!("{}{}", planner_prompt, query)}));

        // Plan phase also needs context management
        // (Planning usually doesn't need much, but let's be safe)

        let base_url = self.get_model_url(ModelRole::Planner).await?;
        let url_props = format!("{base_url}/props");

        let n_ctx = if let Ok(resp) = self.client.get(url_props).send().await {
            let props: Value = resp.json().await.unwrap_or_default();
            props["n_ctx"].as_u64().unwrap_or(8192)
        } else {
            8192
        };

        let mut prompt_tokens = self
            .count_message_tokens(&temp_messages, ModelRole::Planner)
            .await? as u64;
        while prompt_tokens
            > n_ctx
                .saturating_sub(u64::from(PLAN_MAX_TOKENS))
                .saturating_sub(SAFETY_BUFFER)
            && temp_messages.len() > 2
        {
            temp_messages.remove(1);
            prompt_tokens = self
                .count_message_tokens(&temp_messages, ModelRole::Planner)
                .await? as u64;
        }

        let body = json!({
            "messages": &temp_messages,
            "temperature": 0.1,
            "max_tokens": PLAN_MAX_TOKENS,
        });

        let resp: Value = self
            .client
            .post(format!("{base_url}/v1/chat/completions"))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");

        let steps = parse_plan_steps(content, query);

        if steps.is_empty() {
            anyhow::bail!("Planner produced no steps");
        }

        Ok(ExecutionPlan::new(steps))
    }

    /// Validates a plan against the original query to ensure it addresses the request
    pub async fn validate_plan(&mut self, query: &str, plan: &ExecutionPlan) -> Result<bool> {
        const VALIDATION_MAX_TOKENS: u32 = 512;

        let plan_summary = plan
            .steps
            .iter()
            .enumerate()
            .map(|(i, step)| format!("{}. {}", i + 1, step.description))
            .collect::<Vec<_>>()
            .join("\n");

        let validation_prompt = format!(
            "Review this plan for the given query. Answer ONLY 'YES' or 'NO' followed by a brief reason.\n\n\
            Query: {query}\n\n\
            Plan:\n{plan_summary}\n\n\
            Does this plan directly address the query without doing unnecessary work?\n\
            - If the query asks to 'use review_code tool', the plan should just call review_code, not implement changes.\n\
            - If the query asks to 'review X', the plan should analyze X, not modify it.\n\
            - If the query asks to review a 'module' or 'package', the plan should review ALL files in that module, not just main.rs.\n\
            - If the query asks for analysis/review only, the plan should not include implementation steps.\n\n\
            Answer (YES/NO and why):"
        );

        let temp_messages = vec![
            json!({"role": "system", "content": "You are a plan validator. Answer YES or NO followed by a brief reason."}),
            json!({"role": "user", "content": validation_prompt}),
        ];

        let base_url = self.get_model_url(ModelRole::Chat).await?;

        let body = json!({
            "messages": &temp_messages,
            "temperature": 0.1,
            "max_tokens": VALIDATION_MAX_TOKENS,
        });

        let resp: Value = self
            .client
            .post(format!("{base_url}/v1/chat/completions"))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .trim();

        let is_valid = content.to_uppercase().starts_with("YES");

        if !is_valid {
            eprintln!("⚠️  Plan validation failed: {content}");
        }

        Ok(is_valid)
    }

    pub async fn stream(
        &mut self,
        user_input: &str,
        sampling: SamplingConfig,
        is_debug: bool,
        interaction: &dyn crate::ui::interface::InteractionHandler,
    ) -> Result<StreamOutcome> {
        self.ensure_alive().await?;

        // Context-Aware Retrieval
        if let Some(ref indexer) = self.indexer
            && let Ok(results) = indexer.search(user_input, 3)
            && !results.is_empty()
        {
            use std::fmt::Write as _;
            let mut context_msg = String::from("Relevant code snippets from the project:\n\n");
            for (path, content) in results {
                let _ = writeln!(context_msg, "--- {path} ---");
                let snippet: String = content.lines().take(30).collect::<Vec<_>>().join("\n");
                let _ = writeln!(context_msg, "{snippet}");
                if content.lines().count() > 30 {
                    let _ = writeln!(context_msg, "\n...(truncated)");
                }
                let _ = writeln!(context_msg, "\n\n");
            }
            self.messages
                .push(json!({"role": "system", "content": context_msg}));
        }

        self.messages
            .push(json!({"role": "user", "content": user_input}));

        let mut total_tool_calls_executed = 0usize;

        loop {
            let max_tokens = self
                .manage_context(&sampling, is_debug, interaction)
                .await?;
            let body = self.build_request_body(&sampling, max_tokens);
            let base_url = self.get_model_url(ModelRole::Chat).await?;
            let url = format!("{base_url}/v1/chat/completions");

            let resp = self.client.post(&url).json(&body).send().await?;
            if !resp.status().is_success() {
                return self
                    .handle_error_response(resp, &url, &body, is_debug, interaction)
                    .await;
            }

            let (full_content, tool_calls) =
                self.process_stream(resp, interaction, is_debug).await?;

            if tool_calls.is_empty() {
                self.messages
                    .push(json!({"role": "assistant", "content": full_content}));
                // interaction.print_stream_end() already handled the newline for streaming text
                return Ok(StreamOutcome {
                    response: full_content,
                    tool_calls_executed: total_tool_calls_executed,
                });
            }

            let executed = self
                .handle_tool_calls(full_content, tool_calls.clone(), is_debug, interaction)
                .await?;
            total_tool_calls_executed += executed;
        }
    }

    async fn manage_context(
        &mut self,
        sampling: &SamplingConfig,
        is_debug: bool,
        interaction: &dyn crate::ui::interface::InteractionHandler,
    ) -> Result<u32> {
        const SAFETY_BUFFER: u64 = 1536;
        let configured_context = sampling.context.unwrap_or(8192);
        let runtime_context = self.get_runtime_context_limit(ModelRole::Chat).await;
        let context = configured_context.min(u32::try_from(runtime_context).unwrap_or(u32::MAX));
        let mut max_tokens = sampling.max_tokens.unwrap_or(4096);

        let context_u64 = u64::from(context);
        let messages = self.messages.clone();
        let message_tokens = self
            .count_message_tokens(&messages, ModelRole::Chat)
            .await? as u64;
        // The request always sends full tools schema; include it in budgeting to avoid
        // server-side "request exceeded context size" errors.
        let tools_payload = serde_json::to_string(&*TOOLS_JSON).unwrap_or_default();
        let tools_tokens = self
            .count_tokens(&tools_payload, ModelRole::Chat)
            .await
            .unwrap_or(tools_payload.len() / 4) as u64;
        let mut prompt_tokens = message_tokens + tools_tokens;

        let reserved_for_generation = u64::from(max_tokens).max(context_u64 / 4);
        let max_allowed_prompt = context_u64
            .saturating_sub(reserved_for_generation)
            .saturating_sub(SAFETY_BUFFER);

        while prompt_tokens > max_allowed_prompt && self.messages.len() > 2 {
            self.messages.remove(1);
            let messages = self.messages.clone();
            let reduced_message_tokens = self
                .count_message_tokens(&messages, ModelRole::Chat)
                .await? as u64;
            prompt_tokens = reduced_message_tokens + tools_tokens;
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
                "Context usage: ~{total_estimated}/{context_u64} tokens ({used_percent}%). Messages: {}. Runtime n_ctx: {runtime_context}.",
                self.messages.len(),
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
    ) -> Result<StreamOutcome> {
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
        is_debug: bool,
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

                if is_debug && (!delta["content"].is_null() || !delta["tool_calls"].is_null()) {
                    interaction.print_debug(&format!(
                        "Stream Delta: {}",
                        serde_json::to_string(delta).unwrap_or_default()
                    ));
                }

                if let Some(content) = delta["content"].as_str() {
                    if !has_started_printing {
                        interaction.print_stream_start();
                        has_started_printing = true;
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
    ) -> Result<usize> {
        self.messages.push(json!({
            "role": "assistant",
            "content": if full_content.is_empty() { json!(null) } else { json!(full_content) },
            "tool_calls": tool_calls
        }));

        let mut executed = 0usize;
        for tool_call in tool_calls {
            let id = tool_call["id"].as_str().unwrap_or("");
            let name = tool_call["function"]["name"].as_str().unwrap_or("");
            let args_raw = tool_call["function"]["arguments"].as_str().unwrap_or("{}");

            let args_parsed: Value = serde_json::from_str(args_raw).unwrap_or_else(|_| json!({}));

            let tool_result = execute_tool(name, args_parsed, self.indexer.as_ref()).await;

            let result_str = match tool_result {
                Ok(r) => r,
                Err(e) => {
                    let err_msg = format!("Tool error: {e}");
                    interaction.print_debug(&err_msg);
                    err_msg
                }
            };

            // Display tool result to user (but not for read_file or read_multiple_files to avoid console spam)
            let should_print = !matches!(name, "read_file" | "read_multiple_files");
            if should_print {
                interaction.print_response(&result_str);
            }
            interaction.print_stream_start(); // Resume the line prefix for the assistant's potential summary

            if is_debug {
                let preview = if result_str.chars().count() > 200 {
                    let truncated: String = result_str.chars().take(200).collect();
                    format!("{}... ({} bytes total)", truncated, result_str.len())
                } else {
                    result_str.clone()
                };
                interaction.print_debug(&format!("Tool Result: {preview}"));
            }

            self.messages.push(json!({
                "role": "tool",
                "tool_call_id": id,
                "content": result_str
            }));
            executed += 1;
        }
        Ok(executed)
    }

    async fn ensure_alive(&mut self) -> Result<()> {
        let handle = self
            .model_manager
            .get_or_start_model(
                ModelRole::Chat,
                self.config
                    .models
                    .get(&ModelRole::Chat)
                    .ok_or_else(|| anyhow!("Chat model not configured"))?,
                &self.config.sampling,
                self.config.debug.unwrap_or(false),
            )
            .await?;

        if let ModelBackend::LocalLlama { port, .. } = handle.backend
            && !is_port_open("127.0.0.1", port).await
        {
            return Err(anyhow!("llama-server not responding on port {port}"));
        }
        Ok(())
    }

    async fn count_tokens(&mut self, content: &str, role: ModelRole) -> Result<usize> {
        let base_url = self.get_model_url(role).await?;
        let url = format!("{base_url}/tokenize");

        let mut attempts = 0;
        let resp: Value = loop {
            let res = self
                .client
                .post(&url)
                .json(&json!({ "content": content }))
                .send()
                .await;

            match res {
                Ok(resp) => {
                    if resp.status().as_u16() == 503 && attempts < 5 {
                        attempts += 1;
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                    if !resp.status().is_success() {
                        // Some backends might not support /tokenize, return approximate
                        return Ok(content.len() / 4);
                    }
                    break resp.json().await?;
                }
                Err(_) if attempts < 5 => {
                    attempts += 1;
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                Err(e) => return Err(e.into()),
            }
        };

        resp["tokens"]
            .as_array()
            .map(Vec::len)
            .ok_or_else(|| anyhow!("Invalid response from /tokenize"))
    }

    async fn count_message_tokens(&mut self, messages: &[Value], role: ModelRole) -> Result<usize> {
        // Approximate overhead for chat format (im_start, role, im_end, etc.)
        // For Qwen-style: <|im_start|>role\ncontent<|im_end|>\n
        let mut total = 0;
        for msg in messages {
            let role_name = msg["role"].as_str().unwrap_or("");
            let content = msg["content"].as_str().unwrap_or("");

            // Handle content being a string or potentially null (for assistant tool calls without content)
            if !content.is_empty() {
                total += self.count_tokens(content, role).await?;
            }

            total += self.count_tokens(role_name, role).await?;

            // If it's a tool call, we should also account for the tool calls and results
            if let Some(tool_calls) = msg["tool_calls"].as_array() {
                for call in tool_calls {
                    let call_str = call.to_string();
                    total += self.count_tokens(&call_str, role).await?;
                }
            }

            total += 8; // Constant overhead per message
        }
        Ok(total)
    }
}
