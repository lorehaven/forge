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

use crate::llm::{Llm, ollama::OllamaModel};
use model::switchboard_client::SwitchboardClient;
use model::vllm_client::{VllmConfig, VllmModel};

pub mod backend;
pub mod config;
pub mod engine;
pub mod llm;
pub mod model;
pub mod ui;

use config::workflow::{AgentConfig, Workflow};
use engine::executor::{AgentNode, execute};
use model::{ModelManager, SwitchboardManager, SwitchboardModelConfig};

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

/// How long welder waits for the configured backend (ollama daemon, or a
/// reachable switchboard) to come up before giving up. Without a bound, a
/// misconfigured `switchboard_url` or a backend that never starts leaves the
/// process spinning silently forever with no feedback at all.
const BACKEND_READY_TIMEOUT_SECS: u64 = 30;

async fn init_runtime() -> anyhow::Result<()> {
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

/// Resolve each agent's `[models.NAME]` (or inline `[agent.vllm]`) entry,
/// deduplicated by referenced model name. Shared by the vllm and switchboard
/// backends, which both key their instances off the same workflow schema.
fn resolve_model_configs(
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

fn extract_vllm_instances(workflow: &Workflow) -> anyhow::Result<Vec<model::ModelInstanceConfig>> {
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

fn extract_switchboard_instances(
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
fn build_agents<F>(
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
async fn build_vllm_agents(workflow: &Workflow) -> anyhow::Result<HashMap<String, AgentNode>> {
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
fn build_ollama_agents(workflow: &Workflow) -> anyhow::Result<HashMap<String, AgentNode>> {
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
/// instead of spawning `vllm serve` itself. Once an instance is running,
/// welder still talks to it directly over the same OpenAI-compatible chat
/// endpoint the local vllm backend uses.
async fn build_switchboard_agents(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml_src: &str) -> Workflow {
        toml::from_str(toml_src).expect("valid workflow toml")
    }

    #[test]
    fn extract_vllm_instances_requires_url() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"
            vllm_model = "m"

            [models.m]
            dtype = "float16"
            "#,
        );

        let err = extract_vllm_instances(&workflow).unwrap_err();
        assert!(err.to_string().contains("missing 'url'"));
    }

    #[test]
    fn extract_vllm_instances_resolves_referenced_model() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"
            vllm_model = "m"

            [models.m]
            url = "127.0.0.1:8000"
            "#,
        );

        let instances = extract_vllm_instances(&workflow).unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].url, "127.0.0.1:8000");
        assert_eq!(instances[0].model, "m");
    }

    #[test]
    fn extract_vllm_instances_errors_on_unknown_model_ref() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"
            vllm_model = "does-not-exist"

            [models.other]
            url = "127.0.0.1:8000"
            "#,
        );

        let err = extract_vllm_instances(&workflow).unwrap_err();
        assert!(err.to_string().contains("not defined in [models]"));
    }

    #[test]
    fn extract_switchboard_instances_does_not_require_url() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"
            vllm_model = "m"

            [models.m]
            quantization = "Q4_K_M"
            task = "generate"
            "#,
        );

        let instances = extract_switchboard_instances(&workflow).unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].model, "m");
        assert_eq!(instances[0].quantization.as_deref(), Some("Q4_K_M"));
        assert_eq!(instances[0].task.as_deref(), Some("generate"));
    }

    #[test]
    fn extract_switchboard_instances_dedupes_by_model_name() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "shared"
            instruction = "do things"
            vllm_model = "shared"
            children = ["b"]

            [[agent]]
            name = "b"
            model = "shared"
            instruction = "do things"
            vllm_model = "shared"

            [models.shared]
            dtype = "float16"
            "#,
        );

        let instances = extract_switchboard_instances(&workflow).unwrap();
        assert_eq!(instances.len(), 1);
    }

    #[test]
    fn agents_without_model_config_are_skipped_by_resolve() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"
            "#,
        );

        assert!(resolve_model_configs(&workflow).unwrap().is_empty());
    }

    #[test]
    fn resolve_model_configs_errors_when_models_table_is_missing_entirely() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"
            vllm_model = "m"
            "#,
        );

        let err = resolve_model_configs(&workflow).unwrap_err();
        assert!(err.to_string().contains("no [models] table is defined"));
    }

    #[test]
    fn resolve_model_configs_uses_the_inline_agent_vllm_block() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"

            [agent.vllm]
            url = "127.0.0.1:9000"
            "#,
        );

        let resolved = resolve_model_configs(&workflow).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].1.url.as_deref(), Some("127.0.0.1:9000"));
    }

    #[test]
    fn extract_vllm_instances_dedupes_by_model_and_url() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "shared"
            instruction = "do things"
            vllm_model = "shared"
            children = ["b"]

            [[agent]]
            name = "b"
            model = "shared"
            instruction = "do things"
            vllm_model = "shared"

            [models.shared]
            url = "127.0.0.1:8000"
            "#,
        );

        let instances = extract_vllm_instances(&workflow).unwrap();
        assert_eq!(instances.len(), 1);
    }

    #[test]
    fn extract_vllm_instances_keeps_separate_urls_for_the_same_model_name() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"

            [agent.vllm]
            url = "127.0.0.1:8000"

            [[agent]]
            name = "b"
            model = "m"
            instruction = "do things"

            [agent.vllm]
            url = "127.0.0.1:8001"
            "#,
        );

        let instances = extract_vllm_instances(&workflow).unwrap();
        assert_eq!(instances.len(), 2);
    }

    #[derive(Debug)]
    struct NoopLlm;

    #[async_trait::async_trait]
    impl llm::Llm for NoopLlm {
        fn name(&self) -> &'static str {
            "noop"
        }

        async fn generate_content(
            &self,
            _request: llm::LlmRequest,
        ) -> anyhow::Result<llm::LlmResponse> {
            unreachable!("not exercised by these tests")
        }
    }

    #[test]
    fn build_agents_applies_defaults_when_the_workflow_omits_them() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"
            "#,
        );

        let agents = build_agents(&workflow, |_cfg| Ok(Arc::new(NoopLlm) as Arc<dyn Llm>)).unwrap();
        let agent = &agents["a"];
        assert_eq!(agent.max_tool_steps, 8);
        assert!(agent.children.is_empty());
        assert!(agent.tools.is_empty());
        assert!(agent.run_cmd_allowlist.is_empty());
        assert!((agent.temperature - llm::DEFAULT_TEMPERATURE).abs() < f32::EPSILON);
        assert_eq!(agent.max_tokens, llm::DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn build_agents_keeps_explicit_overrides() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"
            children = ["b"]
            tools = ["read_file"]
            max_tool_steps = 3
            temperature = 0.2
            max_tokens = 512
            "#,
        );

        let agents = build_agents(&workflow, |_cfg| Ok(Arc::new(NoopLlm) as Arc<dyn Llm>)).unwrap();
        let agent = &agents["a"];
        assert_eq!(agent.max_tool_steps, 3);
        assert_eq!(agent.children, vec!["b".to_string()]);
        assert_eq!(agent.tools, vec!["read_file".to_string()]);
        assert!((agent.temperature - 0.2).abs() < f32::EPSILON);
        assert_eq!(agent.max_tokens, 512);
    }

    #[test]
    fn build_agents_propagates_a_model_construction_error() {
        let workflow = parse(
            r#"
            [root]
            name = "a"

            [[agent]]
            name = "a"
            model = "m"
            instruction = "do things"
            "#,
        );

        let err = build_agents(&workflow, |_cfg| Err(anyhow::anyhow!("boom"))).unwrap_err();
        assert!(err.to_string().contains("boom"));
    }
}
