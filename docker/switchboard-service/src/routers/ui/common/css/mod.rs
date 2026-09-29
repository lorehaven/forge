use quench_starter::actix::routers::ui::common::css;
use quench_web::prelude::CssRule;
// `css` is framework-agnostic; it lives under `actix::` only by history.

pub mod estimates;
pub mod header;
pub mod models;
pub mod shared;
pub mod vllm;

pub fn ensure_switchboard_css() {
    forge_ui::write_css("switchboard.css", &switchboard_css_rules());
}

fn switchboard_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules.extend(forge_ui::css_rules());
    rules.extend(css::meta_rules());
    rules.extend(header::header_rules());
    rules.extend(shared::shared_dashboard_rules());
    rules.extend(models::models_rules());
    rules.extend(estimates::estimates_modal_rules());
    rules.extend(vllm::vllm_rules());
    rules
}
