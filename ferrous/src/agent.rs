use crate::tools::execute_tool;
use anyhow::{anyhow, Result};
use colored::Colorize;
use reqwest::Client;
use serde_json::{Value, json};
use std::io::Write;
use std::process::Child;
use std::sync::{Arc, Mutex};
use crate::llm::is_port_open;

pub struct Agent {
    client: Client,
    pub messages: Vec<Value>,
    pub server: Option<Arc<Mutex<Child>>>,
    port: u16,
}

impl Agent {
    /// Create new Agent + spawn llama-server
    pub async fn new(model: &str, port: u16, debug: bool) -> Result<Self> {
        use crate::llm::spawn_server;

        let server_handle = spawn_server(model, port, debug).await?;

        Ok(Self {
            client: Client::new(),
            messages: vec![json!({
                "role": "system",
                "content": "You are a helpful coding assistant. You can ONLY use these tools when necessary: read_file, write_file, list_directory, get_directory_tree. Do NOT invent new tool names. If no tool is needed, just respond normally. Always stay in the current working directory."
            })],
            server: Some(server_handle),
            port,
        })
    }

    /// Connect to existing server (no spawn)
    pub async fn connect_only(port: u16) -> Result<Self> {
        use crate::llm::connect_only;

        connect_only(port).await?;

        Ok(Self {
            client: Client::new(),
            messages: vec![json!({
                "role": "system",
                "content": "You are a helpful coding assistant. You can ONLY use these tools when necessary: read_file, write_file, list_directory, get_directory_tree. Do NOT invent new tool names. If no tool is needed, just respond normally. Always stay in the current working directory."
            })],
            server: None,
            port,
        })
    }

    pub async fn stream(
        &mut self,
        user_input: &str,
        temperature: f32,
        top_p: f32,
        top_k: i32,
        max_tokens: u32,
    ) -> Result<String> {
        self.ensure_alive().await?;

        self.messages
            .push(json!({"role": "user", "content": user_input}));

        let tools = vec![
            json!({ "type": "function", "function": {
                "name": "read_file", "description": "Read file content",
                "parameters": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }
            }}),
            json!({ "type": "function", "function": {
                "name": "write_file", "description": "Create directories if needed and write/overwrite file",
                "parameters": { "type": "object", "properties": { "path": { "type": "string" }, "content": { "type": "string" } }, "required": ["path", "content"] }
            }}),
            json!({ "type": "function", "function": {
                "name": "list_directory", "description": "List immediate files/directories in path",
                "parameters": { "type": "object", "properties": { "path": { "type": "string" } } }
            }}),
            json!({ "type": "function", "function": {
                "name": "get_directory_tree", "description": "Recursively list directory tree",
                "parameters": { "type": "object", "properties": { "path": { "type": "string" } } }
            }}),
        ];

        loop {
            let body = json!({
                "messages": &self.messages,
                "tools": &tools,
                "tool_choice": "auto",
                "temperature": temperature,
                "top_p": top_p,
                "top_k": top_k,
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
                        match execute_tool(name, args) {
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
