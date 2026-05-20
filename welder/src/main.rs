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
use model::vllm_client::{VllmConfig, VllmModel};

pub mod backend;
pub mod config;
pub mod engine;
pub mod model;
pub mod ui;

use config::workflow::Workflow;
use engine::executor::{AgentNode, execute};
use model::ModelManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if handle_version_flag() {
        return Ok(());
    }

    init_runtime().await?;

    let workflow_path = get_workflow_path()?;
    println!("[welder] Loading workflow from: {workflow_path}");
    let workflow = load_workflow(&workflow_path)?;
    println!("[welder] ✓ Workflow loaded");

    println!("[welder] Initializing model manager...");
    let vllm_instances = extract_vllm_instances(&workflow)?;
    if vllm_instances.is_empty() {
        println!("[welder] ⚠ No vLLM configuration found in workflow agents");
        println!("[welder] Agents must have [agent.vllm] configuration");
        Err(anyhow::anyhow!(
            "vLLM not configured for any agents in workflow"
        ))
    } else {
        println!("[welder] Found {} vLLM instance(s)", vllm_instances.len());
        for inst in &vllm_instances {
            println!("[welder]   - {} on {}", inst.model, inst.url);
        }

        let vllm_cfg = model::VllmConfig {
            timeout_seconds: workflow
                .vllm
                .as_ref()
                .and_then(|v| v.timeout_seconds)
                .unwrap_or(300),
        };

        let mut model_manager = ModelManager::new(vllm_cfg);
        for inst in vllm_instances {
            model_manager.register(inst);
        }
        model_manager.initialize().await?;
        println!("[welder] ✓ Model manager ready");

        println!("[welder] Building agents...");
        let agents = build_agents(&workflow, &model_manager)?;
        println!("[welder] ✓ {} agent(s) ready", agents.len());
        for agent_name in agents.keys() {
            println!("[welder]   - {agent_name}");
        }

        println!("[welder] Starting REPL...");
        run_repl(&workflow, &workflow_path, &agents).await
    }
}

fn handle_version_flag() -> bool {
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next()
        && (arg == "--version" || arg == "-V")
    {
        println!("welder {}", env!("CARGO_PKG_VERSION"));
        return true;
    }
    false
}

fn get_workflow_path() -> anyhow::Result<String> {
    let mut args = std::env::args().skip(1);
    args.next()
        .map_or_else(|| Err(anyhow::anyhow!("Usage: welder <workflow.toml>")), Ok)
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

fn extract_vllm_instances(workflow: &Workflow) -> anyhow::Result<Vec<model::ModelInstanceConfig>> {
    let mut instances = std::collections::HashMap::new();
    let empty_models = std::collections::HashMap::new();
    let model_defs = workflow.models.as_ref().unwrap_or(&empty_models);

    for agent_cfg in &workflow.agent {
        // Resolve vllm config: either direct vllm block or reference to models section
        let vllm_cfg = if let Some(ref model_ref) = agent_cfg.vllm_model {
            model_defs.get(model_ref).ok_or_else(|| {
                anyhow::anyhow!(
                    "Agent '{}' references model '{}' which is not defined in [models]",
                    agent_cfg.name,
                    model_ref
                )
            })?
        } else if let Some(ref vllm) = agent_cfg.vllm {
            vllm
        } else {
            continue; // No vllm config for this agent
        };

        let key = (agent_cfg.model.clone(), vllm_cfg.url.clone());

        instances
            .entry(key)
            .or_insert_with(|| model::ModelInstanceConfig {
                model: agent_cfg.model.clone(),
                url: vllm_cfg.url.clone(),
                model_path: vllm_cfg.model_path.clone(),
                dtype: vllm_cfg.dtype.clone().unwrap_or_else(|| "auto".to_string()),
                max_model_len: vllm_cfg.max_model_len,
                gpu_memory_utilization: vllm_cfg.gpu_memory_utilization.unwrap_or(0.9),
                tensor_parallel_size: vllm_cfg.tensor_parallel_size.unwrap_or(1),
            });
    }

    Ok(instances.into_values().collect())
}

fn build_agents(
    workflow: &Workflow,
    model_manager: &ModelManager,
) -> anyhow::Result<HashMap<String, AgentNode>> {
    let mut agents = HashMap::new();

    for cfg in &workflow.agent {
        let model: Arc<dyn Llm> = match config::CONFIG.backend.kind.as_str() {
            "vllm" => {
                let model_url = model_manager.get_url(&cfg.model).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Agent '{}' references model '{}' but no vLLM config found for it",
                        cfg.name,
                        cfg.model
                    )
                })?;

                let mut config = VllmConfig::new(&cfg.model);
                config.host = format!("http://{model_url}");
                Arc::new(VllmModel::new(config)?)
            }
            "ollama" => {
                let ollama_url = config::CONFIG
                    .backend
                    .ollama_url
                    .clone()
                    .unwrap_or_else(|| "127.0.0.1:11434".to_string());

                let mut config = OllamaConfig::new(&cfg.model);
                config.host = format!("http://{ollama_url}");
                Arc::new(OllamaModel::new(config)?)
            }
            backend => {
                return Err(anyhow::anyhow!("Unsupported backend: {backend}"));
            }
        };

        agents.insert(
            cfg.name.clone(),
            AgentNode {
                instruction: cfg.instruction.clone(),
                model,
                children: cfg.children.clone().unwrap_or_default(),
                tools: cfg.tools.clone().unwrap_or_default(),
                max_tool_steps: cfg.max_tool_steps.unwrap_or(8),
                run_cmd_allowlist: cfg.run_cmd_allowlist.clone().unwrap_or_default(),
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
    ui::print_workflow_header(workflow_path, &workflow.root.name, agents);

    loop {
        ui::print_prompt();
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("exit") {
            break;
        }

        let result = execute(&workflow.root.name, input.to_string(), agents, 0).await?;
        ui::print_answer(&result);
    }

    Ok(())
}
