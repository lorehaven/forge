use serde::{Deserialize, Serialize};
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
    pub fn name(&self) -> &'static str {
        match self {
            Tool::WebSearch => "web_search",
            Tool::WebFetch => "web_fetch",
            Tool::Calculator => "calculator",
            Tool::FileOps => "file_ops",
            Tool::FileSearch => "file_search",
            Tool::FileList => "file_list",
            Tool::Command => "command",
            Tool::CodeExecutor => "code_executor",
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

fn default_timeout() -> u64 {
    60
}

impl CapabilityProfile {
    pub fn new(name: &str, description: &str, tools: &[Tool]) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            enabled_tools: tools.iter().copied().collect(),
            default_timeout_secs: 60,
            tool_configs: std::collections::HashMap::new(),
        }
    }

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

    pub fn is_enabled(&self, tool: Tool) -> bool {
        self.enabled_tools.contains(&tool)
    }

    pub fn enabled_tool_names(&self) -> Vec<&'static str> {
        let mut names: Vec<_> = self.enabled_tools.iter().map(|t| t.name()).collect();
        names.sort();
        names
    }

    pub fn get_timeout_for_tool(&self, tool_name: &str) -> u64 {
        self.tool_configs
            .get(tool_name)
            .map(|cfg| cfg.timeout_secs)
            .unwrap_or(self.default_timeout_secs)
    }

    pub fn requires_confirmation(&self, tool_name: &str) -> bool {
        matches!(tool_name, "command" | "file_ops")
    }
}

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

pub fn list_profiles() -> Vec<CapabilityProfile> {
    vec![
        get_profile("web_assistant").unwrap(),
        get_profile("code_assistant").unwrap(),
        get_profile("cli_agent").unwrap(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_assistant_profile() {
        let profile = get_profile("web_assistant").unwrap();
        assert!(profile.is_enabled(Tool::WebSearch));
        assert!(profile.is_enabled(Tool::WebFetch));
        assert!(profile.is_enabled(Tool::Calculator));
        assert!(!profile.is_enabled(Tool::Command));
        assert!(!profile.is_enabled(Tool::CodeExecutor));
    }

    #[test]
    fn test_code_assistant_profile() {
        let profile = get_profile("code_assistant").unwrap();
        assert!(profile.is_enabled(Tool::CodeExecutor));
        assert!(profile.is_enabled(Tool::FileOps));
        assert!(!profile.is_enabled(Tool::Command));
    }

    #[test]
    fn test_cli_agent_profile() {
        let profile = get_profile("cli_agent").unwrap();
        assert!(profile.is_enabled(Tool::Command));
        assert!(profile.is_enabled(Tool::CodeExecutor));
        assert!(profile.is_enabled(Tool::FileOps));
    }

    #[test]
    fn test_profile_case_insensitive() {
        assert!(get_profile("WEB_ASSISTANT").is_some());
        assert!(get_profile("Web_Assistant").is_some());
    }

    #[test]
    fn test_invalid_profile() {
        assert!(get_profile("nonexistent").is_none());
    }
}
