//! The stylesheet is forge-ui's plus `conveyor.css`; check the combination is written and carries both.

#[test]
fn ensure_conveyor_css_writes_library_and_service_rules() {
    conveyor_service::routers::ui::common::ensure_conveyor_css();
    let css = std::fs::read_to_string("dist/assets/css/conveyor.css").unwrap();
    assert!(css.contains("--fui-brand-600"));
    assert!(css.contains(".project-node"));
    assert!(css.contains(".status-running"));
}
