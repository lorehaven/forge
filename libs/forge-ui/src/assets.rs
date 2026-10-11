//! The library's stylesheet: tokens, base, layout and components as plain CSS files, plus whatever
//! rules a service adds on top (its own page-specific classes).

use quench_web::prelude::CssRule;

const TOKENS: &str = include_str!("../assets/tokens.css");
const BASE: &str = include_str!("../assets/base.css");
const LAYOUT: &str = include_str!("../assets/layout.css");
const COMPONENTS: &str = include_str!("../assets/components.css");
const LEGACY: &str = include_str!("../assets/legacy.css");

/// The library CSS followed by `rules`, in cascade order.
pub fn stylesheet(rules: &[CssRule]) -> String {
    let mut css = [TOKENS, BASE, LAYOUT, COMPONENTS, LEGACY].join("\n");
    for rule in rules {
        css.push('\n');
        css.push_str(&rule.render());
    }
    css
}

/// The library CSS followed by `extra_css` (a service's own stylesheet text) and then `rules`.
pub fn stylesheet_with(extra_css: &str, rules: &[CssRule]) -> String {
    let mut css = stylesheet(&[]);
    css.push('\n');
    css.push_str(extra_css);
    for rule in rules {
        css.push('\n');
        css.push_str(&rule.render());
    }
    css
}

/// Writes the library CSS plus a service's own `extra_css` to `dist/assets/css/<file_name>`.
pub fn write_css_with(file_name: &str, extra_css: &str) {
    let _ = std::fs::create_dir_all("dist/assets/css");
    let _ = std::fs::write(
        format!("dist/assets/css/{file_name}"),
        stylesheet_with(extra_css, &[]),
    );
}

/// Writes the library CSS plus `rules` to `dist/assets/css/<file_name>`, where the shell's stylesheet link points.
pub fn write_css(file_name: &str, rules: &[CssRule]) {
    let _ = std::fs::create_dir_all("dist/assets/css");
    let _ = std::fs::write(format!("dist/assets/css/{file_name}"), stylesheet(rules));
}

/// Writes an already assembled stylesheet (see [`stylesheet_with`]) to `dist/assets/css/<file_name>`.
pub fn write_stylesheet(file_name: &str, css: &str) {
    let _ = std::fs::create_dir_all("dist/assets/css");
    let _ = std::fs::write(format!("dist/assets/css/{file_name}"), css);
}
