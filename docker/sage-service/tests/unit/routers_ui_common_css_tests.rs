use quench_web::prelude::CssRule;
use sage_service::routers::ui::common::css::{chat, files, initializing, projects};

fn assert_rules_render(rules: &[CssRule]) {
    assert!(!rules.is_empty());
    for rule in rules {
        let rendered = rule.render();
        assert!(!rendered.is_empty());
        assert!(rendered.contains('{'));
        assert!(rendered.contains('}'));
    }
}

#[test]
fn projects_rules_render_without_panicking() {
    assert_rules_render(&projects::projects_rules());
}

#[test]
fn initializing_rules_render_without_panicking() {
    assert_rules_render(&initializing::initializing_rules());
}

#[test]
fn files_rules_render_without_panicking() {
    assert_rules_render(&files::files_rules());
}

#[test]
fn chat_rules_render_without_panicking() {
    assert_rules_render(&chat::chat_rules());
}
