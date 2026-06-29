use quench_starter::actix::routers::ui::common::css;
use quench_web::prelude::CssRule;

pub mod grid;
pub mod meta;
pub mod table;
pub mod tree;
pub mod utility;

pub fn ensure_warehouse_css() {
    let css = warehouse_css_rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");

    let _ = std::fs::create_dir_all("dist/assets/css");
    let _ = std::fs::write("dist/assets/css/warehouse.css", css);
}

fn warehouse_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(utility::utility_rules());
    rules.extend(tree::tree_rules());
    rules.extend(table::table_rules());
    rules.extend(grid::grid_rules());
    rules.extend(meta::meta_rules());
    rules.extend(css::meta_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules
}
