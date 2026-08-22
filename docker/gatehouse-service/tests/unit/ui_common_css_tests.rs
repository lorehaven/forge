use gatehouse_service::ui::common::css::{admin_rules, ensure_gatehouse_css, gatehouse_css_rules};
use quench_web::prelude::CssRule;

#[test]
fn admin_rules_is_non_empty_and_covers_the_admin_selectors() {
    let rules = admin_rules();
    assert!(!rules.is_empty());
    let rendered: String = rules.iter().map(CssRule::render).collect();
    assert!(rendered.contains(".admin-content"));
    assert!(rendered.contains(".admin-danger"));
    assert!(rendered.contains("button.admin-delete"));
}

#[test]
fn gatehouse_css_rules_combines_the_shared_and_admin_rule_sets() {
    let shared = gatehouse_css_rules();
    let admin_only = admin_rules();
    assert!(shared.len() > admin_only.len());
}

#[test]
fn ensure_gatehouse_css_does_not_panic() {
    // `ui::common::shell()` (exercised by several other modules' tests via
    // `render_page`/`ensure_assets`, all sharing this crate's process) calls
    // this same function against the same `dist/assets/css/gatehouse.css`
    // path with no locking of its own, so this test only asserts it runs
    // cleanly - reading the file back here would race those concurrent
    // writers for a file whose content is already verified in-memory by the
    // tests above.
    ensure_gatehouse_css();
}
