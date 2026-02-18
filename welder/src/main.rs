#![forbid(unsafe_code)]
#![deny(clippy::all)]
#![deny(clippy::cargo)]
#![deny(clippy::complexity)]
#![deny(clippy::correctness)]
#![deny(clippy::nursery)]
#![deny(clippy::pedantic)]
#![deny(clippy::perf)]
#![deny(clippy::style)]
#![deny(clippy::suspicious)]
#![deny(missing_debug_implementations)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::multiple_crate_versions)]
#![cfg_attr(test, deny(rust_2018_idioms))]

use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    sync::Arc,
    time::Duration,
};

use adk_core::Llm;
use adk_model::ollama::{OllamaConfig, OllamaModel};

pub mod backend;
pub mod config;
pub mod engine;
pub mod model;

use config::workflow::Workflow;
use engine::executor::{AgentNode, execute};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_runtime().await?;

    let workflow_path = get_workflow_path()?;
    let workflow = load_workflow(&workflow_path)?;
    let agents = build_agents(&workflow)?;

    run_repl(&workflow, &workflow_path, &agents).await
}

fn get_workflow_path() -> anyhow::Result<String> {
    let mut args = std::env::args().skip(1);

    args.next().map_or_else(|| Err(anyhow::anyhow!("Usage: welder <workflow.toml>")), Ok)
}

// ─────────────────────────────────────────────────────────────
// RUNTIME INIT
// ─────────────────────────────────────────────────────────────

async fn init_runtime() -> anyhow::Result<()> {
    std::sync::LazyLock::force(&config::CONFIG);
    std::sync::LazyLock::force(&backend::BACKEND);

    while !backend::BACKEND.is_running() {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    backend::BACKEND.initialized();
    dotenvy::dotenv().ok();

    Ok(())
}

// ─────────────────────────────────────────────────────────────
// LOAD WORKFLOW
// ─────────────────────────────────────────────────────────────

fn load_workflow(path: &str) -> anyhow::Result<Workflow> {
    let toml = fs::read_to_string(path)?;
    let workflow: Workflow = toml::from_str(&toml)?;
    Ok(workflow)
}

// ─────────────────────────────────────────────────────────────
// BUILD AGENTS
// ─────────────────────────────────────────────────────────────

fn build_agents(workflow: &Workflow) -> anyhow::Result<HashMap<String, AgentNode>> {
    let mut agents = HashMap::new();

    for cfg in &workflow.agent {
        let model: Arc<dyn Llm> = Arc::new(OllamaModel::new(OllamaConfig::new(&cfg.model))?);

        agents.insert(
            cfg.name.clone(),
            AgentNode {
                instruction: cfg.instruction.clone(),
                model,
                children: cfg.children.clone().unwrap_or_default(),
            },
        );
    }

    Ok(agents)
}

// ─────────────────────────────────────────────────────────────
// REPL LOOP
// ─────────────────────────────────────────────────────────────

async fn run_repl(
    workflow: &Workflow,
    workflow_path: &str,
    agents: &HashMap<String, AgentNode>,
) -> anyhow::Result<()> {
    println!("Dynamic Workflow Loaded from file: {workflow_path}",);
    println!("Root: {}\n", workflow.root.name);

    println!("Agent Graph:\n");
    print_graph(&workflow.root.name, agents, 0);

    println!("Type 'exit' to quit.\n");

    loop {
        print!("You ▸ ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("exit") {
            break;
        }

        let result = execute(&workflow.root.name, input.to_string(), agents, 0).await?;

        println!("{}", result.trim());
    }

    Ok(())
}

fn print_graph(name: &str, agents: &HashMap<String, AgentNode>, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{indent}- {name}");

    if let Some(node) = agents.get(name) {
        for child in &node.children {
            print_graph(child, agents, depth + 1);
        }
    }
}
