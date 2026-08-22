use crate::support;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use welder::engine::executor::{
    AgentNode, MAX_HISTORY_CHARS, MAX_TOOL_OUTPUT_CHARS, cap_history, default_allowlist_for_stack,
    detect_tech_stack, execute, extract_json, first_sentence, has_extension,
    resolve_run_cmd_allowlist, truncate_for_history,
};
use welder::llm::{Content, Llm, LlmRequest, LlmResponse};

#[test]
fn extract_json_from_raw_object() {
    assert_eq!(
        extract_json(r#"{"action":"final"}"#),
        Some(r#"{"action":"final"}"#.to_string())
    );
}

#[test]
fn extract_json_from_fenced_block() {
    let raw = "```json\n{\"action\":\"final\",\"content\":\"hi\"}\n```";
    assert_eq!(
        extract_json(raw),
        Some("{\"action\":\"final\",\"content\":\"hi\"}".to_string())
    );
}

#[test]
fn extract_json_from_surrounding_prose() {
    let raw = "Sure, here it is: {\"action\":\"final\"} - hope that helps!";
    assert_eq!(extract_json(raw), Some(r#"{"action":"final"}"#.to_string()));
}

#[test]
fn extract_json_returns_none_without_braces() {
    assert_eq!(extract_json("no json here"), None);
}

#[test]
fn first_sentence_splits_on_terminator() {
    assert_eq!(first_sentence("Do the thing. Then stop."), "Do the thing");
    assert_eq!(first_sentence("No terminator here"), "No terminator here");
}

#[test]
fn truncate_for_history_leaves_short_text_untouched() {
    assert_eq!(truncate_for_history("short"), "short");
}

#[test]
fn truncate_for_history_truncates_long_text() {
    let long = "a".repeat(MAX_TOOL_OUTPUT_CHARS + 500);
    let result = truncate_for_history(&long);
    assert!(result.len() < long.len());
    assert!(result.contains("truncated"));
}

#[test]
fn cap_history_leaves_short_history_untouched() {
    let mut history = "short history".to_string();
    cap_history(&mut history);
    assert_eq!(history, "short history");
}

#[test]
fn cap_history_trims_from_the_front() {
    let mut history = "a".repeat(MAX_HISTORY_CHARS + 1000);
    cap_history(&mut history);
    assert!(history.len() < MAX_HISTORY_CHARS + 1000);
    assert!(history.starts_with("...[earlier tool output truncated]"));
}

#[test]
fn has_extension_matches_case_insensitively() {
    assert!(has_extension("main.rs", "rs"));
    assert!(has_extension("Main.RS", "rs"));
    assert!(!has_extension("main.rs", "py"));
    assert!(!has_extension("no-extension", "rs"));
}

#[test]
fn detect_tech_stack_recognizes_rust_by_manifest_and_extension() {
    let files = vec!["Cargo.toml".to_string(), "src/main.rs".to_string()];
    assert_eq!(detect_tech_stack(&files), vec!["rust".to_string()]);
}

#[test]
fn detect_tech_stack_recognizes_node_by_lockfile_and_extension() {
    let files = vec!["package.json".to_string(), "src/index.ts".to_string()];
    assert_eq!(detect_tech_stack(&files), vec!["node".to_string()]);
}

#[test]
fn detect_tech_stack_is_case_insensitive_on_manifest_names() {
    let files = vec!["CARGO.TOML".to_string()];
    assert_eq!(detect_tech_stack(&files), vec!["rust".to_string()]);
}

#[test]
fn detect_tech_stack_returns_sorted_unique_matches_for_a_mixed_repo() {
    let files = vec![
        "Cargo.toml".to_string(),
        "package.json".to_string(),
        "src/main.rs".to_string(),
        "web/index.ts".to_string(),
        "web/other.ts".to_string(),
    ];
    // BTreeSet backing detect_tech_stack sorts and dedupes.
    assert_eq!(
        detect_tech_stack(&files),
        vec!["node".to_string(), "rust".to_string()]
    );
}

#[test]
fn detect_tech_stack_is_empty_for_an_unrecognized_repo() {
    let files = vec!["README.md".to_string(), "LICENSE".to_string()];
    assert!(detect_tech_stack(&files).is_empty());
}

#[test]
fn default_allowlist_for_stack_always_includes_the_common_commands() {
    let allowlist = default_allowlist_for_stack(&["rust".to_string()]);
    assert!(allowlist.contains(&"git status".to_string()));
    assert!(allowlist.contains(&"ls".to_string()));
}

#[test]
fn default_allowlist_for_stack_adds_tech_specific_commands() {
    let allowlist = default_allowlist_for_stack(&["rust".to_string()]);
    assert!(allowlist.contains(&"cargo test".to_string()));
    assert!(!allowlist.contains(&"npm run".to_string()));
}

#[test]
fn default_allowlist_for_stack_combines_multiple_detected_techs() {
    let allowlist = default_allowlist_for_stack(&["rust".to_string(), "node".to_string()]);
    assert!(allowlist.contains(&"cargo test".to_string()));
    assert!(allowlist.contains(&"npm run".to_string()));
}

#[test]
fn default_allowlist_for_stack_allows_every_known_tech_when_nothing_was_detected() {
    // Unknown tech: better to over-allow than to block a legitimate command
    // just because preindexing couldn't identify the stack.
    let allowlist = default_allowlist_for_stack(&[]);
    assert!(allowlist.contains(&"cargo test".to_string()));
    assert!(allowlist.contains(&"npm run".to_string()));
    assert!(allowlist.contains(&"mix test".to_string()));
}

fn agent_node(run_cmd_allowlist: Vec<String>) -> AgentNode {
    AgentNode {
        instruction: String::new(),
        model: Arc::new(NoopLlm),
        children: Vec::new(),
        tools: Vec::new(),
        max_tool_steps: 1,
        run_cmd_allowlist,
        temperature: 0.0,
        max_tokens: 1,
    }
}

#[derive(Debug)]
struct NoopLlm;

#[async_trait::async_trait]
impl Llm for NoopLlm {
    fn name(&self) -> &'static str {
        "noop"
    }

    async fn generate_content(
        &self,
        _request: welder::llm::LlmRequest,
    ) -> anyhow::Result<welder::llm::LlmResponse> {
        unreachable!("not exercised by these tests")
    }
}

#[test]
fn resolve_run_cmd_allowlist_prefers_an_explicit_configured_allowlist() {
    let node = agent_node(vec!["make deploy".to_string()]);
    let resolved = resolve_run_cmd_allowlist(&node, Some(&["Cargo.toml".to_string()]));
    assert_eq!(resolved, vec!["make deploy".to_string()]);
}

#[test]
fn resolve_run_cmd_allowlist_falls_back_to_detected_stack_when_unconfigured() {
    let node = agent_node(Vec::new());
    let resolved = resolve_run_cmd_allowlist(&node, Some(&["Cargo.toml".to_string()]));
    assert!(resolved.contains(&"cargo test".to_string()));
    assert!(!resolved.contains(&"npm run".to_string()));
}

#[test]
fn resolve_run_cmd_allowlist_with_no_index_falls_back_to_every_known_tech() {
    let node = agent_node(Vec::new());
    let resolved = resolve_run_cmd_allowlist(&node, None);
    assert!(resolved.contains(&"cargo test".to_string()));
    assert!(resolved.contains(&"npm run".to_string()));
}

// -----------------------------------------------------------------
// execute / execute_with_tools (via execute, since it's private) - a
// scripted `Llm` that hands back one queued reply per call lets these run
// fully in-process, with no real model or network involved.
// -----------------------------------------------------------------

#[derive(Debug)]
struct ScriptedLlm {
    replies: Mutex<VecDeque<String>>,
}

impl ScriptedLlm {
    fn new(replies: impl IntoIterator<Item = &'static str>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into_iter().map(str::to_string).collect()),
        })
    }
}

#[async_trait::async_trait]
impl Llm for ScriptedLlm {
    fn name(&self) -> &'static str {
        "scripted"
    }

    async fn generate_content(&self, _request: LlmRequest) -> anyhow::Result<LlmResponse> {
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("ScriptedLlm ran out of queued replies");
        Ok(LlmResponse {
            content: Some(Content::new("assistant").with_text(reply)),
        })
    }
}

fn leaf_node(model: Arc<dyn Llm>, tools: Vec<String>) -> AgentNode {
    AgentNode {
        instruction: "do the thing".to_string(),
        model,
        children: Vec::new(),
        tools,
        max_tool_steps: 3,
        run_cmd_allowlist: Vec::new(),
        temperature: 0.0,
        max_tokens: 64,
    }
}

#[tokio::test]
async fn execute_with_no_children_or_tools_returns_the_model_s_reply_directly() {
    let mut agents = HashMap::new();
    agents.insert(
        "root".to_string(),
        leaf_node(ScriptedLlm::new(["the final answer"]), Vec::new()),
    );

    let result = execute("root", "hello".to_string(), &agents, 0)
        .await
        .unwrap();
    assert_eq!(result, "the final answer");
}

#[tokio::test]
async fn execute_errors_when_the_named_agent_is_missing() {
    let agents: HashMap<String, AgentNode> = HashMap::new();
    let err = execute("does-not-exist", "hi".to_string(), &agents, 0)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Agent not found"));
}

#[tokio::test]
async fn execute_routes_to_the_delegate_the_model_names() {
    let mut agents = HashMap::new();
    let mut root = leaf_node(ScriptedLlm::new(["I'll delegate to child"]), Vec::new());
    root.children = vec!["child".to_string()];
    agents.insert("root".to_string(), root);
    agents.insert(
        "child".to_string(),
        leaf_node(ScriptedLlm::new(["child's answer"]), Vec::new()),
    );

    let result = execute("root", "hello".to_string(), &agents, 0)
        .await
        .unwrap();
    assert_eq!(result, "child's answer");
}

#[tokio::test]
async fn execute_falls_back_to_self_when_no_delegate_name_matches() {
    let mut agents = HashMap::new();
    let mut root = leaf_node(
        ScriptedLlm::new(["not a real delegate name", "handled it myself"]),
        Vec::new(),
    );
    root.children = vec!["child".to_string()];
    agents.insert("root".to_string(), root);
    agents.insert(
        "child".to_string(),
        leaf_node(ScriptedLlm::new(["should never be called"]), Vec::new()),
    );

    let result = execute("root", "hello".to_string(), &agents, 0)
        .await
        .unwrap();
    assert_eq!(result, "handled it myself");
}

fn with_temp_cwd() -> (
    std::sync::MutexGuard<'static, ()>,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let guard = support::cwd_lock().lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    (guard, original, dir)
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // single test process; the guard also restores cwd on drop
async fn execute_with_tools_runs_a_tool_then_returns_the_final_answer() {
    let (_guard, original, dir) = with_temp_cwd();
    std::fs::write(dir.path().join("f.txt"), "hello").unwrap();

    let mut agents = HashMap::new();
    agents.insert(
        "root".to_string(),
        leaf_node(
            ScriptedLlm::new([
                r#"{"action":"tool","tool":"list_dir","args":{"path":"."}}"#,
                r#"{"action":"final","content":"listed it"}"#,
            ]),
            vec!["list_dir".to_string()],
        ),
    );

    let result = execute("root", "hello".to_string(), &agents, 0).await;
    std::env::set_current_dir(&original).unwrap();
    assert_eq!(result.unwrap(), "listed it");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // single test process; the guard also restores cwd on drop
async fn execute_with_tools_passes_through_unparseable_model_output() {
    let (_guard, original, dir) = with_temp_cwd();

    let mut agents = HashMap::new();
    agents.insert(
        "root".to_string(),
        leaf_node(
            ScriptedLlm::new(["not json at all"]),
            vec!["list_dir".to_string()],
        ),
    );

    let result = execute("root", "hello".to_string(), &agents, 0).await;
    std::env::set_current_dir(&original).unwrap();
    let _ = dir;
    assert_eq!(result.unwrap(), "not json at all");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // single test process; the guard also restores cwd on drop
async fn execute_with_tools_rejects_a_tool_not_in_the_allowed_list_then_gives_up() {
    let (_guard, original, dir) = with_temp_cwd();

    let mut agents = HashMap::new();
    agents.insert(
        "root".to_string(),
        AgentNode {
            max_tool_steps: 1,
            ..leaf_node(
                ScriptedLlm::new([r#"{"action":"tool","tool":"write_file","args":{}}"#]),
                vec!["list_dir".to_string()],
            )
        },
    );

    let result = execute("root", "hello".to_string(), &agents, 0).await;
    std::env::set_current_dir(&original).unwrap();
    let _ = dir;
    let output = result.unwrap();
    assert!(output.contains("Unable to complete task"));
    assert!(output.contains("not in allowed tools"));
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // single test process; the guard also restores cwd on drop
async fn execute_with_tools_treats_an_unrecognized_action_as_a_history_entry_and_continues() {
    let (_guard, original, dir) = with_temp_cwd();

    let mut agents = HashMap::new();
    agents.insert(
        "root".to_string(),
        AgentNode {
            max_tool_steps: 2,
            ..leaf_node(
                ScriptedLlm::new([
                    r#"{"action":"ponder","content":"hmm"}"#,
                    r#"{"action":"final","content":"ok now"}"#,
                ]),
                vec!["list_dir".to_string()],
            )
        },
    );

    let result = execute("root", "hello".to_string(), &agents, 0).await;
    std::env::set_current_dir(&original).unwrap();
    let _ = dir;
    assert_eq!(result.unwrap(), "ok now");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // single test process; the guard also restores cwd on drop
async fn execute_with_tools_preindexes_the_project_when_index_project_is_an_allowed_tool() {
    let (_guard, original, dir) = with_temp_cwd();
    std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();

    let mut agents = HashMap::new();
    agents.insert(
        "root".to_string(),
        leaf_node(
            ScriptedLlm::new([r#"{"action":"final","content":"done"}"#]),
            vec!["index_project".to_string()],
        ),
    );

    let result = execute("root", "hello".to_string(), &agents, 0).await;
    std::env::set_current_dir(&original).unwrap();
    assert_eq!(result.unwrap(), "done");
}
