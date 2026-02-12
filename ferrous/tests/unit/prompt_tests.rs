use ferrous::config::prompt::{PromptContext, PromptManager};

#[test]
fn test_render_system_fallback() {
    let pm = PromptManager::new().unwrap();
    let ctx = PromptContext {
        date: "2024-01-01".to_string(),
        os: "linux".to_string(),
        project_name: "test".to_string(),
    };
    let rendered = pm.render_system(&ctx).unwrap();
    // It should either use the template or fall back to DEFAULT_PROMPT
    assert!(rendered.contains("You are Ferrous") || rendered.contains("assistant"));
}

#[test]
fn test_render_planner_fallback() {
    let pm = PromptManager::new().unwrap();
    let ctx = PromptContext {
        date: "2024-01-01".to_string(),
        os: "linux".to_string(),
        project_name: "test".to_string(),
    };
    let rendered = pm.render_planner(&ctx).unwrap();
    assert!(rendered.contains("plan") || rendered.contains("steps"));
}
