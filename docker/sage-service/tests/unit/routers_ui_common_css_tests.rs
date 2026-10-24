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

#[quench_test::test]
fn projects_rules_render_without_panicking() {
    assert_rules_render(&projects::projects_rules());
}

#[quench_test::test]
fn initializing_rules_render_without_panicking() {
    assert_rules_render(&initializing::initializing_rules());
}

#[quench_test::test]
fn files_rules_render_without_panicking() {
    assert_rules_render(&files::files_rules());
}

#[quench_test::test]
fn chat_rules_render_without_panicking() {
    assert_rules_render(&chat::chat_rules());
}

#[quench_test::test]
fn sage_stylesheet_carries_library_and_service_rules() {
    let css = sage_service::routers::ui::common::css::sage_stylesheet();
    assert!(css.contains("--fui-brand-600"));
    assert!(css.contains(".sage-layout"));
    assert!(css.contains(".chat-container"));
}

#[quench_test::test]
fn ensure_sage_css_does_not_panic() {
    // Several tests (each its own process under nextest) write the same `dist/assets/css/sage.css`
    // with no locking, so reading it back here would race them; the content is checked in memory above.
    sage_service::routers::ui::common::css::ensure_sage_css();
}
