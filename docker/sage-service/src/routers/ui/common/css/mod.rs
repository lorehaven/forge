use quench_starter::actix::routers::ui::common::css;
use quench_web::prelude::CssRule;

pub mod chat;
pub mod files;
pub mod initializing;
pub mod projects;

pub fn ensure_sage_css() {
    forge_ui::write_css("sage.css", &sage_css_rules());
}

fn sage_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules.extend(forge_ui::css_rules());
    rules.extend(css::meta_rules());
    rules.extend(chat::chat_rules());
    rules.extend(files::files_rules());
    rules.extend(initializing::initializing_rules());
    rules.extend(projects::projects_rules());
    rules
}
