//! Conveyor's stylesheet, composed from the same rule sets the other services build theirs from.

use quench_starter::actix::routers::ui::common::css;
use quench_web::prelude::CssRule;

pub mod nav;
pub mod projects;
pub mod repos;
pub mod runs;
pub mod status;

pub fn ensure_conveyor_css() {
    forge_ui::write_css("conveyor.css", &conveyor_css_rules());
}

fn conveyor_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(css::meta_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules.extend(forge_ui::css_rules());
    rules.extend(status::status_rules());
    rules.extend(runs::runs_rules());
    rules.extend(projects::projects_rules());
    rules.extend(repos::repos_rules());
    rules.extend(nav::nav_rules());
    rules
}
