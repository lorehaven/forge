use adk_core::Llm;
use anyhow::Result;
use serde::Deserialize;
use serde_json::{Value, json};
use std::fmt::{Debug, Formatter};
use std::path::Path;
use std::{
    collections::{BTreeSet, HashMap},
    future::Future,
    pin::Pin,
    sync::Arc,
};

use crate::config::allowlists::{RUN_CMD_COMMON, RUN_CMD_TECH_KEYS, run_cmd_for_tech};
use crate::engine::tools::{run_tool, tool_help};
use crate::model::caller::call_model;

#[derive(Clone)]
pub struct AgentNode {
    pub instruction: String,
    pub model: Arc<dyn Llm>,
    pub children: Vec<String>,
    pub tools: Vec<String>,
    pub max_tool_steps: usize,
    pub run_cmd_allowlist: Vec<String>,
}

impl Debug for AgentNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AgentNode {{ instruction: {}, model name: {} }}",
            self.instruction,
            self.model.name()
        )
    }
}

const RESET: &str = "\x1b[0m";
const CYAN: &str = "\x1b[36m";
const BLUE: &str = "\x1b[34m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const SEP: &str = "────────────────────────────────────────────────────────";

#[must_use]
pub fn execute<'a, S: std::hash::BuildHasher + Sync>(
    name: &'a str,
    input: String,
    agents: &'a HashMap<String, AgentNode, S>,
    depth: usize,
) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
    Box::pin(async move {
        let indent = "  ".repeat(depth);

        println!("\n{CYAN}{SEP}{RESET}\n{BLUE}{indent}Agent ▸ {name}{RESET}\n{CYAN}{SEP}{RESET}");

        let node = agents
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Agent not found: {name}"))?;

        println!("{YELLOW}{indent}Routing...{RESET}");

        let routing_prompt = format!(
            "{}\n\n\
             If delegation is required, respond ONLY with one of:\n\
             {}\n\
             Otherwise respond ONLY with: SELF\n\n\
             User request:\n{input}",
            node.instruction,
            node.children.join(", "),
        );

        let decision = call_model(node.model.clone(), routing_prompt).await?;
        let decision = decision.trim();

        for child in &node.children {
            if decision == child {
                println!("{YELLOW}{indent}Delegating → {child}{RESET}");
                return execute(child, input, agents, depth + 1).await;
            }
        }

        println!("{YELLOW}{indent}Executing task...{RESET}");

        let result = if node.tools.is_empty() {
            let execution_prompt = format!("{}\n\nUser request:\n{input}", node.instruction);
            call_model(node.model.clone(), execution_prompt).await?
        } else {
            execute_with_tools(node, input).await?
        };

        println!("{GREEN}{indent}Completed by {name}{RESET}");

        Ok(result)
    })
}

#[derive(Debug, Deserialize)]
struct ToolResponse {
    action: String,
    content: Option<String>,
    tool: Option<String>,
    args: Option<Value>,
}

async fn execute_with_tools(node: &AgentNode, input: String) -> Result<String> {
    let index = preindex_project(node);
    let effective_run_cmd_allowlist = resolve_run_cmd_allowlist(node, index.file_paths.as_deref());
    let tools_doc = tool_help(&node.tools, &effective_run_cmd_allowlist);

    let mut history = String::new();
    let max_steps = node.max_tool_steps.max(1);

    if let Some(err) = index.error {
        history.push_str("preindex: failed: ");
        history.push_str(&err);
        history.push('\n');
    } else if let Some(summary) = index.summary {
        history.push_str("preindex: ");
        history.push_str(&summary);
        history.push('\n');
    }

    for step in 1..=max_steps {
        let prompt = format!(
            "{instruction}

You are operating with tools.
Return ONLY JSON in one of two formats:
{{\"action\":\"tool\",\"tool\":\"<tool_name>\",\"args\":{{...}}}}
{{\"action\":\"final\",\"content\":\"<answer for user>\"}}

Rules:
- Never wrap JSON in markdown.
- Only use tools from this allowed list: {allowed_tools:?}
- Use tools before making claims about repository contents.
- If a tool fails, adapt and retry or return a concise failure summary.
- Project was pre-indexed before this step. Use that context.

Tool reference:
{tools_doc}

User request:
{input}

Tool interaction history:
{history}

Current step: {step}/{max_steps}
",
            instruction = node.instruction,
            allowed_tools = node.tools,
            tools_doc = tools_doc,
            input = input,
            history = history,
            step = step,
            max_steps = max_steps
        );

        let raw = call_model(node.model.clone(), prompt).await?;
        let parsed = serde_json::from_str::<ToolResponse>(raw.trim());

        let Ok(response) = parsed else {
            // Fallback for models that don't follow tool JSON protocol.
            return Ok(raw);
        };

        if response.action == "final" {
            return Ok(response.content.unwrap_or_default());
        }

        if response.action != "tool" {
            history.push_str("step ");
            history.push_str(&step.to_string());
            history.push_str(": invalid action '");
            history.push_str(&response.action);
            history.push_str("' from model\n");
            continue;
        }

        let tool = response.tool.unwrap_or_default();
        if !node.tools.iter().any(|allowed| allowed == &tool) {
            history.push_str("step ");
            history.push_str(&step.to_string());
            history.push_str(": tool '");
            history.push_str(&tool);
            history.push_str("' is not in allowed tools\n");
            continue;
        }

        let args = response.args.unwrap_or_else(|| json!({}));
        let tool_output = run_tool(&tool, &args, &effective_run_cmd_allowlist);
        match tool_output {
            Ok(result) => {
                history.push_str("step ");
                history.push_str(&step.to_string());
                history.push_str(": tool=");
                history.push_str(&tool);
                history.push_str(" args=");
                history.push_str(&args.to_string());
                history.push_str(" output:\n");
                history.push_str(&result.output);
                history.push('\n');
            }
            Err(err) => {
                history.push_str("step ");
                history.push_str(&step.to_string());
                history.push_str(": tool=");
                history.push_str(&tool);
                history.push_str(" args=");
                history.push_str(&args.to_string());
                history.push_str(" error: ");
                history.push_str(&err.to_string());
                history.push('\n');
            }
        }
    }

    Ok(format!(
        "Unable to complete task after {max_steps} tool step(s). Last context:\n{history}"
    ))
}

#[derive(Debug, Default)]
struct PreindexContext {
    file_paths: Option<Vec<String>>,
    summary: Option<String>,
    error: Option<String>,
}

fn preindex_project(node: &AgentNode) -> PreindexContext {
    if !node.tools.iter().any(|tool| tool == "index_project") {
        return PreindexContext::default();
    }

    match run_tool("index_project", &json!({ "path": "." }), &[]) {
        Ok(result) => {
            let files: Vec<String> = result
                .output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect();

            let detected = detect_tech_stack(&files);
            let tech_summary = if detected.is_empty() {
                "unknown".to_string()
            } else {
                detected.join(", ")
            };

            PreindexContext {
                file_paths: Some(files.clone()),
                summary: Some(format!(
                    "indexed {} file(s); detected tech: {}",
                    files.len(),
                    tech_summary
                )),
                error: None,
            }
        }
        Err(err) => PreindexContext {
            file_paths: None,
            summary: None,
            error: Some(err.to_string()),
        },
    }
}

fn resolve_run_cmd_allowlist(node: &AgentNode, indexed_files: Option<&[String]>) -> Vec<String> {
    if !node.run_cmd_allowlist.is_empty() {
        return node.run_cmd_allowlist.clone();
    }

    let detected = indexed_files.map_or_else(Vec::new, detect_tech_stack);
    default_allowlist_for_stack(&detected)
}

fn detect_tech_stack(files: &[String]) -> Vec<String> {
    let mut tech = BTreeSet::new();

    for path in files {
        let lower = path.to_ascii_lowercase();

        if lower == "cargo.toml" || has_extension(&lower, "rs") {
            tech.insert("rust");
        }
        if lower == "package.json"
            || lower == "pnpm-lock.yaml"
            || lower == "yarn.lock"
            || has_extension(&lower, "ts")
            || has_extension(&lower, "tsx")
            || has_extension(&lower, "js")
            || has_extension(&lower, "jsx")
        {
            tech.insert("node");
        }
        if lower == "pyproject.toml"
            || lower == "requirements.txt"
            || lower == "poetry.lock"
            || has_extension(&lower, "py")
        {
            tech.insert("python");
        }
        if lower == "go.mod" || has_extension(&lower, "go") {
            tech.insert("go");
        }
        if lower == "pom.xml"
            || lower == "build.gradle"
            || lower == "build.gradle.kts"
            || lower == "mvnw"
            || lower == "gradlew"
            || has_extension(&lower, "java")
            || has_extension(&lower, "kt")
        {
            tech.insert("jvm");
        }
        if has_extension(&lower, "csproj")
            || has_extension(&lower, "sln")
            || has_extension(&lower, "cs")
        {
            tech.insert("dotnet");
        }
        if lower == "gemfile" || has_extension(&lower, "rb") {
            tech.insert("ruby");
        }
        if lower == "composer.json" || has_extension(&lower, "php") {
            tech.insert("php");
        }
        if lower == "mix.exs" || has_extension(&lower, "ex") || has_extension(&lower, "exs") {
            tech.insert("elixir");
        }
        if lower == "package.swift" || has_extension(&lower, "swift") {
            tech.insert("swift");
        }
        if lower == "cmakelists.txt"
            || lower == "makefile"
            || has_extension(&lower, "c")
            || has_extension(&lower, "cc")
            || has_extension(&lower, "cpp")
            || has_extension(&lower, "h")
            || has_extension(&lower, "hpp")
        {
            tech.insert("cpp");
        }
    }

    tech.into_iter().map(ToOwned::to_owned).collect()
}

fn default_allowlist_for_stack(detected: &[String]) -> Vec<String> {
    let mut allowlist = BTreeSet::new();
    for cmd in RUN_CMD_COMMON {
        allowlist.insert((*cmd).to_string());
    }

    if detected.is_empty() {
        // Unknown tech: allow all configured command prefixes from all known stacks.
        for tech in RUN_CMD_TECH_KEYS {
            for cmd in run_cmd_for_tech(tech) {
                allowlist.insert((*cmd).to_string());
            }
        }
        return allowlist.into_iter().collect();
    }

    let mut selected = BTreeSet::new();
    for cmd in RUN_CMD_COMMON {
        selected.insert((*cmd).to_string());
    }

    for tech in detected {
        for cmd in run_cmd_for_tech(tech) {
            selected.insert((*cmd).to_string());
        }
    }

    selected.into_iter().collect()
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
}
