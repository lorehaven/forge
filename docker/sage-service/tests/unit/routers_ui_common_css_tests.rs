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

#[test]
fn ensure_sage_css_writes_library_and_service_rules() {
    sage_service::routers::ui::common::css::ensure_sage_css();
    let css = std::fs::read_to_string("dist/assets/css/sage.css").unwrap();
    assert!(css.contains("--fui-brand-600"));
    assert!(css.contains(".sage-layout"));
    assert!(css.contains(".chat-container"));
}
