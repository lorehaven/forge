//! The stylesheet is forge-ui's plus `switchboard.css`; check the combination is written and carries both.

#[test]
fn ensure_switchboard_css_writes_library_and_service_rules() {
    switchboard_service::routers::ui::common::ensure_switchboard_css();
    let css = std::fs::read_to_string("dist/assets/css/switchboard.css").unwrap();
    assert!(css.contains("--fui-brand-600"));
    assert!(css.contains(".sb-toolbar"));
}
