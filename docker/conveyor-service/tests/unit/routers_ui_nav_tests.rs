//! Unit tests for `routers/ui/common/nav.rs`: the slide-out drawer.
//!
//! There is no `NavPanelBuilder`-style locale/theme picker in here at all -
//! this pins that down, and that the drawer carries exactly the two entries
//! conveyor's UI has (the pipelines/repositories home page and the
//! credentials preview page), not palantir's larger, per-section list.

use conveyor_service::routers::ui::common::nav::panel;

#[test]
fn the_drawer_carries_no_select_at_all() {
    let html = panel().render();
    assert!(!html.contains("<select"));
}

#[test]
fn the_drawer_has_exactly_the_home_and_credentials_entries() {
    let html = panel().render();
    assert_eq!(html.matches("side-nav-bar-entry").count(), 2);
    assert!(html.contains("ui_home_button"));
    assert!(html.contains("ui_nav_credentials"));
    assert!(html.contains("/home"));
    assert!(html.contains("/credentials"));
}
