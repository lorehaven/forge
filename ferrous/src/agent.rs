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

static PROMPT: &str = r#"You are a helpful coding assistant.
You can use these tools when necessary: read_file, write_file, list_directory, get_directory_tree, create_directory, file_exists, replace_in_file, execute_shell_command.

For execute_shell_command you may ONLY run cargo commands that start with one of these prefixes:
  cargo check, cargo fmt, cargo clippy, cargo build, cargo run, cargo test, cargo bench, cargo doc, cargo metadata, cargo tree, cargo audit, cargo +nightly ...

NEVER run rm, mv, cp, git push, curl, wget, sudo, or any destructive/unsafe commands — they will be rejected.

Never use git push, git pull, git rebase, git merge or any command that talks to remote repositories.
Only use git_status, git_diff, git_add, git_commit.

Always stay inside the current working directory."#;

pub struct Agent {
    client: Client,
    pub messages: Vec<Value>,
    pub server: Option<Arc<Mutex<Child>>>,
    port: u16,
}

impl Agent {
    /// Create a new Agent and spawn llama-server
    pub async fn new(model: &str, port: u16, debug: bool) -> Result<Self> {
        use crate::llm::spawn_server;

        let server_handle = spawn_server(model, port, debug).await?;

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

    pub async fn stream(
        &mut self,
        user_input: &str,
        temperature: f32,
        top_p: f32,
        min_p: f32,
        top_k: i32,
        repeat_penalty: f32,
        max_tokens: u32,
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
                "top_p": top_p,
                "min_p": min_p,
                "top_k": top_k,
                "repeat_penalty": repeat_penalty,
                "max_tokens": max_tokens,
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
