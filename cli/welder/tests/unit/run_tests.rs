use std::sync::Arc;
use welder::config::workflow::Workflow;
use welder::llm::{self, Llm};
use welder::run::{
    build_agents, build_ollama_agents, extract_switchboard_instances, extract_vllm_instances,
    load_workflow, resolve_model_configs,
};

fn parse(toml_src: &str) -> Workflow {
    toml::from_str(toml_src).expect("valid workflow toml")
}

const SIMPLE_WORKFLOW_TOML: &str = r#"
[root]
name = "a"

[[agent]]
name = "a"
model = "m"
instruction = "do things"
"#;

#[test]
fn load_workflow_reads_and_parses_a_toml_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workflow.toml");
    std::fs::write(&path, SIMPLE_WORKFLOW_TOML).unwrap();

    let workflow = load_workflow(path.to_str().unwrap()).unwrap();
    assert_eq!(workflow.root.name, "a");
    assert_eq!(workflow.agent.len(), 1);
}

#[test]
fn load_workflow_errors_for_a_missing_file() {
    let err = load_workflow("/does/not/exist/workflow.toml").unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn load_workflow_errors_for_invalid_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workflow.toml");
    std::fs::write(&path, "not = [valid").unwrap();

    assert!(load_workflow(path.to_str().unwrap()).is_err());
}

#[test]
fn build_ollama_agents_builds_one_agent_node_per_workflow_agent() {
    // `OllamaModel::new` is a pure constructor (no network call - see
    // `llm/ollama.rs`), so this is safe to run for real without a live
    // ollama daemon.
    let workflow = parse(SIMPLE_WORKFLOW_TOML);
    let agents = build_ollama_agents(&workflow).unwrap();
    assert_eq!(agents.len(), 1);
    assert!(agents.contains_key("a"));
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

#[test]
fn get_workflow_path_errors_with_no_args() {
    let err = welder::run::get_workflow_path_from(std::iter::empty()).unwrap_err();
    assert!(err.to_string().contains("Usage: welder"));
}

#[test]
fn get_workflow_path_returns_the_first_argument() {
    let path =
        welder::run::get_workflow_path_from(vec!["workflow.toml".to_string()].into_iter()).unwrap();
    assert_eq!(path, "workflow.toml");
}

#[test]
fn handle_version_flag_is_false_without_a_version_flag() {
    assert!(!welder::run::handle_version_flag_from(std::iter::empty()));
    assert!(!welder::run::handle_version_flag_from(
        vec!["workflow.toml".to_string()].into_iter()
    ));
}

#[test]
fn handle_version_flag_is_true_for_dash_dash_version_or_dash_v() {
    assert!(welder::run::handle_version_flag_from(
        vec!["--version".to_string()].into_iter()
    ));
    assert!(welder::run::handle_version_flag_from(
        vec!["-V".to_string()].into_iter()
    ));
}
