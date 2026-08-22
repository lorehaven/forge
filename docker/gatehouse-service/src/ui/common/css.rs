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

/// The shared sets the other services compose, plus the admin pages' own rows.
///
/// The forms need no rules here: `style.css` already styles `form`, `input`,
/// `select`, `button` and `form .error` for the whole estate, and overriding them
/// is what made these pages look like a different product. What is local is the
/// user list and the permission matrix - layout for a table of people, which no
/// other service in the estate has.
pub fn gatehouse_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(css::meta_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules.extend(admin_rules());
    rules
}

pub fn admin_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".admin-content").property("width", "100%"),
        CssRule::new(".admin-container")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "1rem")
            .property("max-width", "56rem")
            .property("width", "100%")
            .property("margin", "0 auto"),
        CssRule::new(".admin-panel").property("width", "100%"),
        // One row per user: name and roles on the left, grants in the middle, the
        // edit link pinned right.
        CssRule::new(".admin-row")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "1rem")
            .property("padding", "0.5rem 0")
            .property("border-bottom", "0.1rem solid var(--bs-gray-800)")
            .child(
                CssRule::new(".admin-row-main")
                    .property("display", "flex")
                    .property("align-items", "baseline")
                    .property("gap", "0.5rem")
                    .property("flex", "0 0 40%")
                    .property("min-width", "0"),
            )
            .child(
                CssRule::new(".admin-row-grants")
                    .property("flex", "1 1 auto")
                    .property("min-width", "0")
                    .property("color", "var(--bs-gray-400)")
                    .property("font-size", "0.9rem")
                    .property("overflow-wrap", "anywhere"),
            ),
        CssRule::new(".admin-username").property("font-weight", "600"),
        CssRule::new(".admin-roles")
            .property("color", "var(--bs-gray-500)")
            .property("font-size", "0.85rem"),
        // "you" next to your own row, so the two rules about acting on yourself
        // are predictable rather than surprising.
        CssRule::new(".admin-you")
            .property("padding", "0.05rem 0.4rem")
            .property("border-radius", "0.2rem")
            .property("background-color", "var(--bs-gray-700)")
            .property("font-size", "0.75rem")
            .property("text-transform", "uppercase"),
        CssRule::new(".admin-grant-all").property("color", "var(--bs-green, #4caf50)"),
        CssRule::new(".admin-grant-none").property("color", "var(--bs-gray-600)"),
        CssRule::new("a.button.admin-edit")
            .property("margin-left", "auto")
            .property("white-space", "nowrap"),
        CssRule::new("a.button.admin-back").property("align-self", "flex-start"),
        CssRule::new(".admin-section-title")
            .property("margin-top", "0.5rem")
            .property("font-weight", "600"),
        // Lifecycle/security status rows: a label, a value, and an optional
        // action button pinned right - the same left/middle/right shape
        // `.admin-row` uses for the user list, at a smaller scale.
        CssRule::new(".admin-status-row")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.75rem")
            .property("padding", "0.3rem 0")
            .child(
                CssRule::new("form")
                    .property("margin-left", "auto")
                    .property("width", "auto"),
            ),
        // The matrix: a label column that does not shrink, and a column of
        // action checkboxes that wraps rather than overflowing when a service
        // declares more of them than fit on one line.
        CssRule::new(".admin-matrix")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.4rem")
            .child(
                CssRule::new(".admin-matrix-row")
                    .property("display", "grid")
                    .property(
                        "grid-template-columns",
                        "minmax(8rem, 14rem) minmax(0, 1fr)",
                    )
                    .property("align-items", "center")
                    .property("gap", "0.75rem"),
            )
            .child(
                CssRule::new(".admin-matrix-actions")
                    .property("display", "flex")
                    .property("flex-wrap", "wrap")
                    .property("gap", "0.25rem 1rem"),
            )
            .child(
                CssRule::new(".admin-matrix-action")
                    .property("display", "flex")
                    .property("align-items", "center")
                    .property("gap", "0.35rem")
                    .child(
                        CssRule::new("label")
                            .property("margin", "0")
                            .property("font-weight", "400"),
                    ),
            ),
        CssRule::new(".admin-service")
            .property("margin", "0")
            .property("overflow-wrap", "anywhere"),
        CssRule::new(".admin-hint")
            .property("color", "var(--bs-gray-500)")
            .property("font-size", "0.85rem")
            .property("margin", "0.25rem 0"),
        CssRule::new(".admin-notice")
            .property("padding", "0.6rem 0.9rem")
            .property("border-radius", "0.3rem")
            .property("margin", "0")
            .child(CssRule::new("&.ok").property("background-color", "var(--bs-gray-800)"))
            .child(CssRule::new("&.error").property("background-color", "var(--bs-gray-700)")),
        CssRule::new(".admin-mono")
            .property("font-family", "monospace")
            .property("word-break", "break-all")
            .property("background-color", "var(--bs-gray-800)")
            .property("padding", "0.5rem 0.75rem")
            .property("border-radius", "0.3rem"),
        CssRule::new(".admin-danger").child(
            CssRule::new("button.admin-delete")
                .property("background-color", "var(--bs-red, #b3261e)")
                .property("border-color", "var(--bs-red, #b3261e)"),
        ),
    ]
}
