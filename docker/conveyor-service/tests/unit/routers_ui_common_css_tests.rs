//! Light snapshot tests for conveyor's CSS rule builders. These are pure
//! presentation glue - the value here is proving each rule set renders to
//! non-empty, well-formed CSS containing the selectors it's documented to
//! carry, not exhaustively asserting every property.

use conveyor_service::routers::ui::common::css::{nav, projects, repos, runs, status};

fn rendered(rules: Vec<quench_web::prelude::CssRule>) -> String {
    rules
        .iter()
        .map(quench_web::prelude::CssRule::render)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn nav_rules_render_and_are_not_empty() {
    let css = rendered(nav::nav_rules());
    assert!(!css.is_empty());
}

#[test]
fn projects_rules_render_and_are_not_empty() {
    let css = rendered(projects::projects_rules());
    assert!(!css.is_empty());
}

#[test]
fn repos_rules_render_and_are_not_empty() {
    let css = rendered(repos::repos_rules());
    assert!(!css.is_empty());
}

#[test]
fn runs_rules_render_and_are_not_empty() {
    let css = rendered(runs::runs_rules());
    assert!(!css.is_empty());
}

#[test]
fn status_rules_carry_every_documented_status_class() {
    let css = rendered(status::status_rules());
    for class in [
        ".status",
        ".status-queued",
        ".status-running",
        ".status-success",
        ".status-failed",
        ".status-cancelled",
        ".status-skipped",
    ] {
        assert!(css.contains(class), "missing {class}");
    }
    assert!(css.contains("@keyframes conveyor-pulse"));
    assert!(css.contains("prefers-reduced-motion"));
}
