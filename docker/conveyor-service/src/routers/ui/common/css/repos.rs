//! The repository admin pages: list, create, edit and delete.
//!
//! Adapted from gatehouse's `.admin-*` rules (`docker/gatehouse-service/src/
//! ui/common/css.rs`) - same shapes (a row list, a notice banner, a
//! danger-zone button), conveyor just doesn't have them under these class
//! names yet. Forms need no rules here: `style.css` already styles `form`,
//! `input`, `select`, `button` and `form .error` for the whole estate.

use quench_web::prelude::CssRule;

pub fn repos_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".repos-content").property("width", "100%"),
        CssRule::new(".repos-container")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "1rem")
            .property("max-width", "56rem")
            .property("width", "100%")
            .property("margin", "0 auto"),
        CssRule::new(".repos-panel").property("width", "100%"),
        CssRule::new("a.button.repos-back").property("align-self", "flex-start"),
        // One row per repository: name and project on the left, provider,
        // branch and state in the middle, the edit link pinned right.
        CssRule::new(".repos-row")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "1rem")
            .property("padding", "0.5rem 0")
            .property("border-bottom", "0.1rem solid var(--bs-gray-800)")
            .child(
                CssRule::new(".repos-row-main")
                    .property("display", "flex")
                    .property("flex-direction", "column")
                    .property("gap", "0.15rem")
                    .property("flex", "0 0 40%")
                    .property("min-width", "0"),
            )
            .child(
                CssRule::new(".repos-row-meta")
                    .property("display", "flex")
                    .property("align-items", "center")
                    .property("gap", "0.75rem")
                    .property("flex", "1 1 auto")
                    .property("min-width", "0")
                    .property("font-size", "0.9rem"),
            ),
        CssRule::new(".repos-repo-name").property("font-weight", "600"),
        CssRule::new(".repos-project-path")
            .property("color", "var(--bs-gray-500)")
            .property("font-size", "0.85rem"),
        CssRule::new("a.button.repos-edit")
            .property("margin-left", "auto")
            .property("white-space", "nowrap"),
        CssRule::new(".repos-checkbox-row")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.5rem")
            .child(
                CssRule::new("label")
                    .property("margin", "0")
                    .property("font-weight", "400"),
            ),
        CssRule::new(".repos-hint")
            .property("color", "var(--bs-gray-500)")
            .property("font-size", "0.85rem")
            .property("margin", "0.25rem 0"),
        CssRule::new(".repos-notice")
            .property("padding", "0.6rem 0.9rem")
            .property("border-radius", "0.3rem")
            .property("margin", "0")
            .child(CssRule::new("&.ok").property("background-color", "var(--bs-gray-800)"))
            .child(CssRule::new("&.error").property("background-color", "var(--bs-gray-700)")),
        CssRule::new(".repos-danger").child(
            CssRule::new("button.repos-delete")
                .property("background-color", "var(--bs-red, #b3261e)")
                .property("border-color", "var(--bs-red, #b3261e)"),
        ),
    ]
}
