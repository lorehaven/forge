//! The stylesheet is forge-ui's plus `conveyor.css`.

#[test]
fn conveyor_stylesheet_carries_library_and_service_rules() {
    let css = conveyor_service::routers::ui::common::conveyor_stylesheet();
    assert!(css.contains("--fui-brand-600"));
    assert!(css.contains(".project-node"));
    assert!(css.contains(".status-running"));
}

#[test]
fn ensure_conveyor_css_does_not_panic() {
    // Tests run as separate processes writing the same `dist/assets/css/conveyor.css` unlocked,
    // so reading it back would race them; the content is checked in memory above.
    conveyor_service::routers::ui::common::ensure_conveyor_css();
}
