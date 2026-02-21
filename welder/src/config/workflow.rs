use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Workflow {
    pub root: Root,
    pub agent: Vec<AgentConfig>,
}

#[derive(Debug, Deserialize)]
pub struct Root {
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub model: String,
    pub instruction: String,
    pub children: Option<Vec<String>>,
    pub tools: Option<Vec<String>>,
    pub max_tool_steps: Option<usize>,
    pub run_cmd_allowlist: Option<Vec<String>>,
}
