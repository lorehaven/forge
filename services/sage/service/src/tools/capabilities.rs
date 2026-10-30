use quench_serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    WebSearch,
    WebFetch,
    Calculator,
    FileOps,
    FileSearch,
    FileList,
    Command,
    CodeExecutor,
}

impl Tool {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::WebSearch => "web_search",
            Self::WebFetch => "web_fetch",
            Self::Calculator => "calculator",
            Self::FileOps => "file_ops",
            Self::FileSearch => "file_search",
            Self::FileList => "file_list",
            Self::Command => "command",
            Self::CodeExecutor => "code_executor",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProfile {
    pub name: String,
    pub description: String,
    pub enabled_tools: HashSet<Tool>,
    #[serde(default = "default_timeout")]
    pub default_timeout_secs: u64,
    #[serde(default)]
    pub tool_configs: std::collections::HashMap<String, ToolConfig>,
}

const fn default_timeout() -> u64 {
    60
}

impl CapabilityProfile {
    #[must_use]
    pub fn new(name: &str, description: &str, tools: &[Tool]) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            enabled_tools: tools.iter().copied().collect(),
            default_timeout_secs: 60,
            tool_configs: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_timeouts(mut self, default: u64, overrides: &[(&str, u64)]) -> Self {
        self.default_timeout_secs = default;
        for (tool_name, timeout) in overrides {
            self.tool_configs.insert(
                tool_name.to_string(),
                ToolConfig {
                    timeout_secs: *timeout,
                },
            );
        }
        self
    }

    #[must_use]
    pub fn is_enabled(&self, tool: Tool) -> bool {
        self.enabled_tools.contains(&tool)
    }

    #[must_use]
    pub fn enabled_tool_names(&self) -> Vec<&'static str> {
        let mut names: Vec<_> = self.enabled_tools.iter().map(Tool::name).collect();
        names.sort_unstable();
        names
    }

    #[must_use]
    pub fn get_timeout_for_tool(&self, tool_name: &str) -> u64 {
        self.tool_configs
            .get(tool_name)
            .map_or(self.default_timeout_secs, |cfg| cfg.timeout_secs)
    }

    #[must_use]
    pub fn requires_confirmation(&self, tool_name: &str) -> bool {
        matches!(tool_name, "command" | "file_ops")
    }
}

#[must_use]
pub fn get_profile(name: &str) -> Option<CapabilityProfile> {
    match name.to_lowercase().as_str() {
        "web_assistant" => Some(
            CapabilityProfile::new(
                "web_assistant",
                "Web browsing and research capabilities only",
                &[
                    Tool::WebSearch,
                    Tool::WebFetch,
                    Tool::Calculator,
                    Tool::FileSearch,
                    Tool::FileList,
                ],
            )
            .with_timeouts(
                30,
                &[
                    ("web_search", 30),
                    ("web_fetch", 20),
                    ("calculator", 5),
                    ("file_search", 30),
                    ("file_list", 10),
                ],
            ),
        ),
        "code_assistant" => Some(
            CapabilityProfile::new(
                "code_assistant",
                "Code execution with web access, no shell commands",
                &[
                    Tool::WebSearch,
                    Tool::WebFetch,
                    Tool::Calculator,
                    Tool::CodeExecutor,
                    Tool::FileOps,
                    Tool::FileSearch,
                    Tool::FileList,
                ],
            )
            .with_timeouts(
                60,
                &[
                    ("web_search", 30),
                    ("web_fetch", 20),
                    ("calculator", 5),
                    ("code_executor", 90),
                    ("file_ops", 15),
                    ("file_search", 30),
                    ("file_list", 10),
                ],
            ),
        ),
        "cli_agent" => Some(
            CapabilityProfile::new(
                "cli_agent",
                "Full CLI access with command execution and file operations",
                &[
                    Tool::WebSearch,
                    Tool::WebFetch,
                    Tool::Calculator,
                    Tool::FileOps,
                    Tool::FileSearch,
                    Tool::FileList,
                    Tool::Command,
                    Tool::CodeExecutor,
                ],
            )
            .with_timeouts(
                120,
                &[
                    ("web_search", 30),
                    ("web_fetch", 20),
                    ("calculator", 5),
                    ("code_executor", 90),
                    ("file_ops", 15),
                    ("file_search", 30),
                    ("file_list", 10),
                    ("command", 120),
                ],
            ),
        ),
        _ => None,
    }
}

#[must_use]
pub fn list_profiles() -> Vec<CapabilityProfile> {
    vec![
        get_profile("web_assistant").unwrap(),
        get_profile("code_assistant").unwrap(),
        get_profile("cli_agent").unwrap(),
    ]
}
