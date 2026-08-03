//! The front page's project tree.
//!
//! Visually the same disclosure `.job`/`details.job` already gives the run
//! page's job list (see `runs.rs`) - a bordered box with a clickable head and
//! no script needed for collapse and expand - just nested arbitrarily deep
//! instead of one level, with each nesting indented under its parent.

use quench_web::prelude::CssRule;

pub fn projects_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".project-tree"),
        CssRule::new(".project-node")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.3rem")
            .property("margin-bottom", "0.75rem")
            .property("overflow", "hidden"),
        CssRule::new(".project-head")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.6rem")
            .property("padding", "0.5rem 0.9rem")
            .property("background-color", "var(--bs-gray-800)")
            .property("cursor", "pointer")
            .property("user-select", "none"),
        CssRule::new(".project-name")
            .property("font-weight", "600")
            .property("text-decoration", "none")
            .child(CssRule::new("&:hover").property("text-decoration", "underline")),
        // An empty leaf's own link - same muted colour it had before it was
        // clickable, just without the browser's default underline until it is
        // actually hovered.
        CssRule::new(".project-leaf-link")
            .property("text-decoration", "none")
            .child(CssRule::new("&:hover").property("text-decoration", "underline")),
        // `<details>` gives collapse and expand with no script; the marker is
        // removed the same way `details.job` removes it.
        CssRule::new("details.project-node > summary")
            .property("list-style", "none")
            .child(CssRule::new("::-webkit-details-marker").property("display", "none")),
        // A nested node's own margin is what draws the tree - each level
        // indents under the one before it, and the last child's margin does
        // not leave a gap before its parent's closing border.
        CssRule::new(".project-children")
            .property("padding", "0 0.9rem 0.9rem")
            .child(
                CssRule::new(".project-node")
                    .property("margin", "0.6rem 0 0")
                    .property("border-color", "var(--bs-gray-700)"),
            ),
        CssRule::new(".project-tree > .project-node:last-child").property("margin-bottom", "0"),
        // ---------------------------------------------------------------
        // Breadcrumb - a scoped page's own header, in place of the plain
        // title the unscoped front page and pipeline list use.
        // ---------------------------------------------------------------
        CssRule::new(".breadcrumb")
            .property("display", "flex")
            .property("align-items", "baseline")
            .property("flex-wrap", "wrap")
            .property("gap", "0.5rem")
            .property("margin", "0")
            .child(
                CssRule::new("a")
                    .property("text-decoration", "none")
                    .child(CssRule::new("&:hover").property("text-decoration", "underline")),
            ),
        CssRule::new(".breadcrumb-sep")
            .property("color", "var(--bs-gray-600)")
            .property("font-weight", "400"),
        CssRule::new(".breadcrumb-current").property("color", "var(--bs-gray-400)"),
    ]
}
