//! Gatehouse's stylesheet, generated from the same rule sets the other
//! services build theirs from - that is what keeps the estate looking like one
//! product rather than four.

use quench_starter::actix::routers::ui::common::css;
use quench_web::prelude::CssRule;

pub fn ensure_gatehouse_css() {
    let rules = gatehouse_css_rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");

    let _ = std::fs::create_dir_all("dist/assets/css");
    let _ = std::fs::write("dist/assets/css/gatehouse.css", rules);
}

/// Exactly the sets the other services compose, and nothing else. The form
/// itself needs no rules here: `style.css` already styles `form`, `input`,
/// `button` and `form .error` for the whole estate, and overriding them is what
/// made this page look like a different product.
fn gatehouse_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(css::meta_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules
}
