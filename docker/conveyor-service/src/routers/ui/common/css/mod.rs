//! Conveyor's stylesheet, composed from the same rule sets the other services
//! build theirs from - that is what keeps the estate looking like one product
//! rather than six.

use quench_starter::actix::routers::ui::common::css;
use quench_web::prelude::CssRule;

pub mod projects;
pub mod runs;
pub mod status;

pub fn ensure_conveyor_css() {
    let rules = conveyor_css_rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");

    let _ = std::fs::create_dir_all("dist/assets/css");
    let _ = std::fs::write("dist/assets/css/conveyor.css", rules);
}

fn conveyor_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(css::meta_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules.extend(status::status_rules());
    rules.extend(runs::runs_rules());
    rules.extend(projects::projects_rules());
    rules
}
