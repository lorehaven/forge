use quench_web::prelude::CssRule;
use warehouse_service::routers::ui::common::css::{ensure_warehouse_css, warehouse_css_rules};

// Light snapshot-style tests for pure presentation glue: these builders are
// all "assemble a `Vec<CssRule>`" with no branching worth exercising
// individually, so the bar here is "renders to non-empty CSS", not asserting
// on the exact rules.
#[test]
fn warehouse_css_rules_is_non_empty_and_renders_without_panicking() {
    let rules = warehouse_css_rules();
    assert!(!rules.is_empty());
    let rendered: String = rules
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!rendered.trim().is_empty());
}

#[test]
fn ensure_warehouse_css_writes_the_stylesheet_without_panicking() {
    ensure_warehouse_css();
    let path = std::path::Path::new("dist/assets/css/warehouse.css");
    // Best-effort: `ensure_warehouse_css` itself swallows I/O errors, so this
    // only asserts it didn't panic; if it did write, the file should be
    // non-empty.
    if let Ok(contents) = std::fs::read_to_string(path) {
        assert!(!contents.trim().is_empty());
    }
}
