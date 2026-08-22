use std::{collections::HashMap, fs, io, io::Write, sync::Arc, time::Duration};

use crate::llm::{Llm, ollama::OllamaModel};
use crate::model::switchboard_client::SwitchboardClient;
use crate::model::vllm_client::{VllmConfig, VllmModel};
use crate::{config, engine, llm, model, ui};

use config::workflow::{AgentConfig, Workflow};
use engine::executor::{AgentNode, execute};
use model::{ModelManager, SwitchboardManager, SwitchboardModelConfig};

/// How long welder waits for the configured backend to come up before
/// giving up.
///
/// Without a bound, a misconfigured `switchboard_url` or a backend that
/// never starts leaves the process spinning silently forever with no
/// feedback at all.
pub const BACKEND_READY_TIMEOUT_SECS: u64 = 30;

#[must_use]
pub fn handle_version_flag_from(mut args: impl Iterator<Item = String>) -> bool {
    if let Some(arg) = args.next()
        && (arg == "--version" || arg == "-V")
    {
        println!("welder {}", env!("CARGO_PKG_VERSION"));
        return true;
    }
    false
}

#[must_use]
pub fn handle_version_flag() -> bool {
    handle_version_flag_from(std::env::args().skip(1))
}

pub fn get_workflow_path_from(mut args: impl Iterator<Item = String>) -> anyhow::Result<String> {
    args.next()
        .map_or_else(|| Err(anyhow::anyhow!("Usage: welder <workflow.toml>")), Ok)
}

pub fn get_workflow_path() -> anyhow::Result<String> {
    get_workflow_path_from(std::env::args().skip(1))
}

pub async fn init_runtime() -> anyhow::Result<()> {
    std::sync::LazyLock::force(&config::CONFIG);
    backend::init()?;

    let max_attempts = BACKEND_READY_TIMEOUT_SECS * 2; // 2 attempts per second (500ms each)
    let mut attempts = 0u64;
    while !backend::get().is_running() {
        if attempts >= max_attempts {
            return Err(anyhow::anyhow!(
                "backend '{}' did not become reachable within {BACKEND_READY_TIMEOUT_SECS}s; check [backend] in .welder/config.toml",
                config::CONFIG.backend.kind
            ));
        }
        attempts += 1;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    backend::get().initialized();
    dotenvy::dotenv().ok();

    Ok(())
}

pub fn load_workflow(path: &str) -> anyhow::Result<Workflow> {
    let toml = fs::read_to_string(path)?;
    let workflow: Workflow = toml::from_str(&toml)?;
    Ok(workflow)
}

use crate::backend;

/// Resolve each agent's `[models.NAME]` (or inline `[agent.vllm]`) entry,
/// deduplicated by referenced model name.
///
/// Shared by the vllm and switchboard backends, which both key their
/// instances off the same workflow schema.
pub fn resolve_model_configs(
    workflow: &Workflow,
) -> anyhow::Result<Vec<(&AgentConfig, &config::workflow::AgentVllmConfig)>> {
    let mut resolved = Vec::new();
    for agent_cfg in &workflow.agent {
        let vllm_cfg = if let Some(ref model_ref) = agent_cfg.vllm_model {
            let model_defs = workflow.models.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Agent '{}' references model '{}' but no [models] table is defined",
                    agent_cfg.name,
                    model_ref
                )
            })?;
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
            continue; // No model config for this agent
        };

        resolved.push((agent_cfg, vllm_cfg));
    }

    Ok(resolved)
}

pub fn extract_vllm_instances(
    workflow: &Workflow,
) -> anyhow::Result<Vec<model::ModelInstanceConfig>> {
    let mut instances = std::collections::HashMap::new();

    for (agent_cfg, vllm_cfg) in resolve_model_configs(workflow)? {
        let url = vllm_cfg.url.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Agent '{}' vllm config is missing 'url' (required for the local vllm backend)",
                agent_cfg.name
            )
        })?;

        let key = (agent_cfg.model.clone(), url.clone());

        instances
            .entry(key)
            .or_insert_with(|| model::ModelInstanceConfig {
                model: agent_cfg.model.clone(),
                url,
                model_path: vllm_cfg.model_path.clone(),
                dtype: vllm_cfg.dtype.clone().unwrap_or_else(|| "auto".to_string()),
                max_model_len: vllm_cfg.max_model_len,
                gpu_memory_utilization: vllm_cfg.gpu_memory_utilization.unwrap_or(0.9),
                tensor_parallel_size: vllm_cfg.tensor_parallel_size.unwrap_or(1),
            });
    }

    Ok(instances.into_values().collect())
}

pub fn extract_switchboard_instances(
    workflow: &Workflow,
) -> anyhow::Result<Vec<SwitchboardModelConfig>> {
    let mut instances = std::collections::HashMap::new();

    for (agent_cfg, vllm_cfg) in resolve_model_configs(workflow)? {
        instances
            .entry(agent_cfg.model.clone())
            .or_insert_with(|| SwitchboardModelConfig {
                model: agent_cfg.model.clone(),
                quantization: vllm_cfg.quantization.clone(),
                dtype: vllm_cfg.dtype.clone(),
                max_model_len: vllm_cfg.max_model_len.and_then(|v| u32::try_from(v).ok()),
                gpu_memory_utilization: vllm_cfg.gpu_memory_utilization,
                limit_mm_per_prompt: vllm_cfg.limit_mm_per_prompt.clone(),
                task: vllm_cfg.task.clone(),
            });
    }

    Ok(instances.into_values().collect())
}

/// Build every agent in the workflow, delegating model construction to
/// `make_model` so each backend only has to say how it turns an
/// [`AgentConfig`] into an [`Llm`] client.
pub fn build_agents<F>(
    workflow: &Workflow,
    mut make_model: F,
) -> anyhow::Result<HashMap<String, AgentNode>>
where
    F: FnMut(&AgentConfig) -> anyhow::Result<Arc<dyn Llm>>,
{
    let mut agents = HashMap::new();

    for cfg in &workflow.agent {
        let model = make_model(cfg)?;

        agents.insert(
            cfg.name.clone(),
            AgentNode {
                instruction: cfg.instruction.clone(),
                model,
                children: cfg.children.clone().unwrap_or_default(),
                tools: cfg.tools.clone().unwrap_or_default(),
                max_tool_steps: cfg.max_tool_steps.unwrap_or(8),
                run_cmd_allowlist: cfg.run_cmd_allowlist.clone().unwrap_or_default(),
                temperature: cfg.temperature.unwrap_or(llm::DEFAULT_TEMPERATURE),
                max_tokens: cfg.max_tokens.unwrap_or(llm::DEFAULT_MAX_TOKENS),
            },
        );
    }

    Ok(agents)
}

/// Local backend: welder spawns and owns a `vllm serve` process per model,
/// on the fixed `host:port` from each `[models.NAME]` entry.
pub async fn build_vllm_agents(workflow: &Workflow) -> anyhow::Result<HashMap<String, AgentNode>> {
    let vllm_instances = extract_vllm_instances(workflow)?;
    if vllm_instances.is_empty() {
        println!("[welder] ⚠ No vLLM configuration found in workflow agents");
        println!("[welder] Agents must have [agent.vllm] configuration");
        return Err(anyhow::anyhow!(
            "vLLM not configured for any agents in workflow"
        ));
    }

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

    build_agents(workflow, |cfg| {
        let model_url = model_manager.get_url(&cfg.model).ok_or_else(|| {
            anyhow::anyhow!(
                "Agent '{}' references model '{}' but no vLLM config found for it",
                cfg.name,
                cfg.model
            )
        })?;

        let mut vllm_config = VllmConfig::new(&cfg.model);
        vllm_config.host = format!("http://{model_url}");
        Ok(Arc::new(VllmModel::new(vllm_config)?) as Arc<dyn Llm>)
    })
}

/// Ollama backend: no instance management at all, just a shared local daemon.
pub fn build_ollama_agents(workflow: &Workflow) -> anyhow::Result<HashMap<String, AgentNode>> {
    let ollama_url = config::CONFIG
        .backend
        .ollama_url
        .clone()
        .unwrap_or_else(|| "127.0.0.1:11434".to_string());

    build_agents(workflow, |cfg| {
        Ok(Arc::new(OllamaModel::new(
            &cfg.model,
            format!("http://{ollama_url}"),
        )?) as Arc<dyn Llm>)
    })
}

/// Switchboard backend: model instances are launched and tracked by a
/// switchboard-service, and welder discovers their `host:port` from it
/// instead of spawning `vllm serve` itself.
///
/// Once an instance is running, welder still talks to it directly over the
/// same OpenAI-compatible chat endpoint the local vllm backend uses.
pub async fn build_switchboard_agents(
    workflow: &Workflow,
) -> anyhow::Result<HashMap<String, AgentNode>> {
    let switchboard_models = extract_switchboard_instances(workflow)?;
    if switchboard_models.is_empty() {
        println!("[welder] ⚠ No model configuration found in workflow agents");
        println!("[welder] Agents must reference a [models.NAME] entry");
        return Err(anyhow::anyhow!(
            "switchboard backend requires at least one model reference in the workflow"
        ));
    }

    println!(
        "[welder] Resolving {} model(s) via switchboard",
        switchboard_models.len()
    );

    let base_url = config::CONFIG
        .backend
        .switchboard_url
        .clone()
        .ok_or_else(|| anyhow::anyhow!("config error: backend.switchboard_url must be set"))?;
    let client =
        SwitchboardClient::new(&base_url, config::CONFIG.backend.switchboard_tls_verify).await?;

    let timeout_seconds = workflow
        .switchboard
        .as_ref()
        .and_then(|s| s.timeout_seconds)
        .unwrap_or(300);

    let mut switchboard_manager = SwitchboardManager::new(client, timeout_seconds);
    for model_cfg in switchboard_models {
        switchboard_manager.register(model_cfg);
    }
    switchboard_manager.initialize().await?;
    println!("[welder] ✓ Switchboard model(s) ready");

    build_agents(workflow, |cfg| {
        let model_url = switchboard_manager.get_url(&cfg.model).ok_or_else(|| {
            anyhow::anyhow!(
                "Agent '{}' references model '{}' but switchboard has no instance for it",
                cfg.name,
                cfg.model
            )
        })?;

        let mut vllm_config = VllmConfig::new(&cfg.model);
        vllm_config.host = format!("http://{model_url}");
        Ok(Arc::new(VllmModel::new(vllm_config)?) as Arc<dyn Llm>)
    })
}

// `agents` is always built by this crate's own `build_agents` (a plain
// `HashMap<String, AgentNode>`), never called with an arbitrary hasher from
// outside - genericizing over `BuildHasher` here buys nothing.
#[allow(clippy::implicit_hasher)]
pub async fn run_repl(
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

pub async fn run() -> anyhow::Result<()> {
    if handle_version_flag() {
        return Ok(());
    }

    init_runtime().await?;

    let workflow_path = get_workflow_path()?;
    println!("[welder] Loading workflow from: {workflow_path}");
    let workflow = load_workflow(&workflow_path)?;
    println!("[welder] ✓ Workflow loaded");

    println!("[welder] Building agents...");
    let agents = match config::CONFIG.backend.kind.as_str() {
        "vllm" => build_vllm_agents(&workflow).await?,
        "ollama" => build_ollama_agents(&workflow)?,
        "switchboard" => build_switchboard_agents(&workflow).await?,
        other => return Err(anyhow::anyhow!("Unsupported backend: {other}")),
    };
    println!("[welder] ✓ {} agent(s) ready", agents.len());
    for agent_name in agents.keys() {
        println!("[welder]   - {agent_name}");
    }

    println!("[welder] Starting REPL...");
    run_repl(&workflow, &workflow_path, &agents).await
}
