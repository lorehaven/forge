//! Light snapshot coverage for the CSS-rule builders under
//! `routers/ui/common/css/*` - pure presentation glue, per this project's
//! convention these get "renders, is non-empty, contains an expected
//! selector" checks rather than deep assertions.

use quench_web::prelude::CssRule;
use switchboard_service::routers::ui::common::css::{estimates, header, models, shared, vllm};

fn rendered(rules: Vec<CssRule>) -> String {
    rules
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn header_rules_render_non_empty_css() {
    let css = rendered(header::header_rules());
    assert!(!css.is_empty());
}

#[test]
fn models_rules_render_and_include_the_dashboard_selector() {
    let css = rendered(models::models_rules());
    assert!(!css.is_empty());
    assert!(css.contains(".models-dashboard-content"));
}

#[test]
fn shared_dashboard_rules_render_non_empty_css() {
    let css = rendered(shared::shared_dashboard_rules());
    assert!(!css.is_empty());
}

#[test]
fn estimates_modal_rules_render_non_empty_css() {
    let css = rendered(estimates::estimates_modal_rules());
    assert!(!css.is_empty());
}

#[test]
fn vllm_rules_render_non_empty_css() {
    let css = rendered(vllm::vllm_rules());
    assert!(!css.is_empty());
}

#[test]
fn ensure_switchboard_css_writes_the_combined_stylesheet_without_panicking() {
    // Writes to `dist/assets/css/switchboard.css` relative to cwd - the same
    // side effect production code already performs on first UI-shell touch
    // (see `common::mod::UI_SHELL_HOME` etc.), not something this test
    // introduces on its own.
    switchboard_service::routers::ui::common::css::ensure_switchboard_css();
    assert!(std::path::Path::new("dist/assets/css/switchboard.css").exists());
}
