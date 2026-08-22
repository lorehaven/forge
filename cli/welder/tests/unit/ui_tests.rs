use std::collections::HashMap;
use std::sync::Arc;
use welder::engine::executor::AgentNode;
use welder::llm::Llm;
use welder::ui::{print_answer, print_backend_banner, print_prompt, print_workflow_header};

#[derive(Debug)]
struct StubLlm;

#[async_trait::async_trait]
impl Llm for StubLlm {
    fn name(&self) -> &'static str {
        "stub-model"
    }

    async fn generate_content(
        &self,
        _request: welder::llm::LlmRequest,
    ) -> anyhow::Result<welder::llm::LlmResponse> {
        unreachable!("ui tests never call the model")
    }
}

fn node(children: Vec<&str>, tool_count: usize) -> AgentNode {
    AgentNode {
        model: Arc::new(StubLlm),
        instruction: "do things".to_string(),
        children: children.into_iter().map(str::to_string).collect(),
        tools: (0..tool_count).map(|i| format!("tool-{i}")).collect(),
        max_tool_steps: 4,
        run_cmd_allowlist: Vec::new(),
        temperature: 0.7,
        max_tokens: 2048,
    }
}

/// These only need to not panic - the actual formatting is terminal
/// escape-code soup not worth asserting on character-for-character.
#[test]
fn print_functions_do_not_panic() {
    print_backend_banner("vllm", "127.0.0.1:8000");
    print_answer("the answer");
    print_answer("  padded with whitespace  \n");
    print_prompt();
}

#[test]
fn print_workflow_header_walks_a_tree_without_panicking() {
    let mut agents = HashMap::new();
    agents.insert("root".to_string(), node(vec!["child"], 2));
    agents.insert("child".to_string(), node(vec![], 0));

    print_workflow_header("workflow.toml", "root", &agents);
}

#[test]
fn print_workflow_header_handles_a_root_missing_from_the_map() {
    let agents: HashMap<String, AgentNode> = HashMap::new();
    print_workflow_header("workflow.toml", "missing-root", &agents);
}
