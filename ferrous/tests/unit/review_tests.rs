use ferrous::core::agent::parse_plan_steps;
use ferrous::tools::review::review_module;
use serde_json::json;
use std::fs;

#[test]
fn test_parse_plan_steps_accepts_common_numbered_formats() {
    let content = "PLAN:\n1. Inspect module\n2) Review files\n3: Summarize findings";
    let steps = parse_plan_steps(content, "review source code");
    assert_eq!(
        steps,
        vec!["Inspect module", "Review files", "Summarize findings"]
    );
}

#[test]
fn test_parse_plan_steps_ignores_heading_and_uses_fallback() {
    let content = "PLAN:\nReview the source code quality and report issues";
    let steps = parse_plan_steps(content, "review source code");
    assert_eq!(
        steps,
        vec!["Review the source code quality and report issues"]
    );
}

#[test]
fn test_review_module_returns_aggregate_summary() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();

    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(
        cwd.join("src/main.rs"),
        "fn main() {\n    // TODO: improve\n    let x = 1;\n}\n",
    )
    .unwrap();
    fs::write(
        cwd.join("src/lib.rs"),
        format!(
            "pub fn long_line() {{\n    let _x = \"{}\";\n}}\n",
            "a".repeat(140)
        ),
    )
    .unwrap();

    let out = review_module(cwd, &json!({ "path": "src" })).unwrap();
    assert!(out.contains("Module Review: src"));
    assert!(out.contains("Files reviewed: 2"));
    assert!(out.contains("Top files to inspect"));
}
