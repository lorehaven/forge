//! Unit tests for `tools/capabilities.rs`.

use sage_service::tools::capabilities::*;

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
