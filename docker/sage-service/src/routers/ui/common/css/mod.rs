use quench_web::prelude::CssRule;

pub mod chat;
pub mod files;
pub mod initializing;
pub mod projects;

/// forge-ui's stylesheet plus sage's own rules.
pub fn sage_stylesheet() -> String {
    forge_ui::stylesheet_with(include_str!("sage.css"), &sage_css_rules())
}

pub fn ensure_sage_css() {
    forge_ui::write_stylesheet("sage.css", &sage_stylesheet());
}

fn sage_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(chat::chat_rules());
    rules.extend(files::files_rules());
    rules.extend(initializing::initializing_rules());
    rules.extend(projects::projects_rules());
    rules
}
