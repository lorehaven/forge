use crate::llm::is_port_open;
use crate::tools::execute_tool;
use anyhow::{Result, anyhow};
use colored::Colorize;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde_json::{Value, json};
use std::io::Write;
use std::process::Child;
use std::sync::{Arc, Mutex};

static TOOLS_JSON: Lazy<Vec<Value>> = Lazy::new(|| {
    let s = include_str!("../config/tools.json");
    serde_json::from_str(s).expect("Invalid tools.json")
});

static PROMPT: &str = r#"
You are Ferrous, an expert Rust developer and autonomous coding agent running in a Rust project.

Your primary goal: help the user write, refactor, fix, improve, and maintain Rust code efficiently and safely.

Core Rules:
- When the user asks to edit, refactor, fix, improve, add, remove, rename, or change ANY code/file — you MUST use write_file or replace_in_file.
  - NEVER just output a code block and say "replace this with that".
  - ALWAYS perform the actual file modification using tools.
- First, use read_file or list_files_recursive to understand the current code.
- For small, targeted changes → prefer replace_in_file (safer, more precise).
- For full-file rewrites or new files → use write_file.
- After any change, ALWAYS use git_diff(path) to show what was modified.
- Never use absolute paths. All paths are relative to the current working directory.
- You can ONLY run cargo commands that start with: cargo check, cargo fmt, cargo clippy, cargo build, cargo run, cargo test, cargo bench, cargo doc, cargo metadata, cargo tree, cargo audit, cargo +nightly ...
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

        let server_handle =
            spawn_server(model, max_tokens, temperature, repeat_penalty, port, debug).await?;

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
                "stream": false,
                "mirostat": mirostat,
                "mirostat_tau": mirostat_tau,
                "mirostat_eta": mirostat_eta,
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

            let choice = &resp["choices"][0];
            let message = choice["message"].clone();

            if let Some(tool_calls) = message["tool_calls"].as_array() {
                self.messages.push(message.clone());

                for call in tool_calls {
                    let func = &call["function"];
                    let name = func["name"].as_str().unwrap_or("");
                    let args_str = func["arguments"].as_str().unwrap_or("{}");

                    if let Ok(args) = serde_json::from_str::<Value>(args_str) {
                        match execute_tool(name, args).await {
                            Ok(result) => {
                                self.messages.push(json!({
                                    "role": "tool",
                                    "tool_call_id": call["id"].as_str().unwrap_or(""),
                                    "name": name,
                                    "content": result
                                }));
                            }
                            Err(e) => {
                                self.messages.push(json!({
                                    "role": "tool",
                                    "tool_call_id": call["id"].as_str().unwrap_or(""),
                                    "name": name,
                                    "content": format!("Tool error: {}", e)
                                }));
                            }
                        }
                    }
                }
                continue;
            } else {
                let content = message["content"].as_str().unwrap_or("").to_string();
                self.messages.push(message);

                // Simulated typewriter
                print!("{}", "│ ".dimmed());
                std::io::stdout().flush()?;

                const DELAY_MS: u64 = 6;
                for c in content.chars() {
                    print!("{}", c);
                    std::io::stdout().flush()?;
                    tokio::time::sleep(tokio::time::Duration::from_millis(DELAY_MS)).await;
                }
                println!();

                return Ok(content);
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
