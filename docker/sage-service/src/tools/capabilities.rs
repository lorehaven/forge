use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    WebSearch,
    WebFetch,
    Calculator,
    FileOps,
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
            Tool::Command => "command",
            Tool::CodeExecutor => "code_executor",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProfile {
    pub name: String,
    pub description: String,
    pub enabled_tools: HashSet<Tool>,
}

impl CapabilityProfile {
    pub fn new(name: &str, description: &str, tools: &[Tool]) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            enabled_tools: tools.iter().copied().collect(),
        }
    }

    pub fn is_enabled(&self, tool: Tool) -> bool {
        self.enabled_tools.contains(&tool)
    }

    pub fn enabled_tool_names(&self) -> Vec<&'static str> {
        let mut names: Vec<_> = self.enabled_tools.iter().map(|t| t.name()).collect();
        names.sort();
        names
    }
}

pub fn get_profile(name: &str) -> Option<CapabilityProfile> {
    match name.to_lowercase().as_str() {
        "web_assistant" => Some(CapabilityProfile::new(
            "web_assistant",
            "Web browsing and research capabilities only",
            &[Tool::WebSearch, Tool::WebFetch, Tool::Calculator],
        )),
        "code_assistant" => Some(CapabilityProfile::new(
            "code_assistant",
            "Code execution with web access, no shell commands",
            &[
                Tool::WebSearch,
                Tool::WebFetch,
                Tool::Calculator,
                Tool::CodeExecutor,
                Tool::FileOps,
            ],
        )),
        "cli_agent" => Some(CapabilityProfile::new(
            "cli_agent",
            "Full CLI access with command execution and file operations",
            &[
                Tool::WebSearch,
                Tool::WebFetch,
                Tool::Calculator,
                Tool::FileOps,
                Tool::Command,
                Tool::CodeExecutor,
            ],
        )),
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
