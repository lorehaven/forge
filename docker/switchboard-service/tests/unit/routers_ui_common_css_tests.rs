//! The stylesheet is forge-ui's plus `switchboard.css`.

#[quench_test::test]
fn switchboard_stylesheet_carries_library_and_service_rules() {
    let css = switchboard_service::routers::ui::common::switchboard_stylesheet();
    assert!(css.contains("--fui-brand-600"));
    assert!(css.contains(".sb-toolbar"));
}

#[quench_test::test]
fn ensure_switchboard_css_does_not_panic() {
    // Tests run as separate processes writing the same `dist/assets/css/switchboard.css` unlocked,
    // so reading it back would race them; the content is checked in memory above.
    switchboard_service::routers::ui::common::ensure_switchboard_css();
}
