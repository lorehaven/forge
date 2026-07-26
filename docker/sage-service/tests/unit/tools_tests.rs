//! Unit tests for `tools/mod.rs`.

use sage_service::tools::*;

#[test]
fn test_tool_definitions_valid_json() {
    let defs = get_tool_definitions_for_prompt();
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&defs);
    assert!(parsed.is_ok(), "Tool definitions should be valid JSON");
}
