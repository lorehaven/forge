use quench_starter::actix::routers::ui::common::css;
use quench_web::prelude::CssRule;

pub mod grid;
pub mod manage;
pub mod meta;
pub mod table;
pub mod tree;
pub mod utility;

/// The rendered CSS is a deterministic function of fixed rule data, so
/// writing it more than once is always redundant - guarded by a `Once`
/// rather than just calling `fs::write` every time so that several `UI_SHELL_*`
/// `LazyLock`s (or a shell and a direct test of this function) racing to
/// initialize concurrently can't interleave two writes to the same path and
/// have a reader observe a half-written file in between.
pub fn ensure_warehouse_css() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let css = warehouse_css_rules()
            .iter()
            .map(CssRule::render)
            .collect::<Vec<_>>()
            .join("\n");

        let _ = std::fs::create_dir_all("dist/assets/css");
        let _ = std::fs::write("dist/assets/css/warehouse.css", css);
    });
}

pub fn warehouse_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(utility::utility_rules());
    rules.extend(tree::tree_rules());
    rules.extend(table::table_rules());
    rules.extend(grid::grid_rules());
    rules.extend(manage::manage_rules());
    rules.extend(meta::meta_rules());
    rules.extend(css::meta_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules
}
