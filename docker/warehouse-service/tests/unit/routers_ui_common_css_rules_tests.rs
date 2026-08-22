use quench_web::prelude::CssRule;
use warehouse_service::routers::ui::common::css::{
    grid::grid_rules, meta::meta_rules, table::table_rules, tree::tree_rules,
    utility::utility_rules,
};

fn assert_renders(rules: Vec<CssRule>) {
    assert!(!rules.is_empty());
    let rendered: String = rules
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!rendered.trim().is_empty());
}

#[test]
fn grid_rules_is_non_empty_and_renders_without_panicking() {
    assert_renders(grid_rules());
}

#[test]
fn meta_rules_is_non_empty_and_renders_without_panicking() {
    assert_renders(meta_rules());
}

#[test]
fn table_rules_is_non_empty_and_renders_without_panicking() {
    assert_renders(table_rules());
}

#[test]
fn tree_rules_is_non_empty_and_renders_without_panicking() {
    assert_renders(tree_rules());
}

#[test]
fn utility_rules_is_non_empty_and_renders_without_panicking() {
    assert_renders(utility_rules());
}
