use quench_web::prelude::CssRule;

pub fn ensure_workbench_css() {
    forge_ui::write_css("workbench.css", &workbench_css_rules());
}

/// Only what is specific to workbench: the board, its cards and the dependency lists.
/// Everything else (forms, buttons, panels, modals) comes from forge-ui.
fn workbench_css_rules() -> Vec<CssRule> {
    let mut rules = board_rules();
    rules.extend(link_rules());
    rules
}

fn board_rules() -> Vec<CssRule> {
    vec![
        // One scrollbar: the board scrolls sideways itself, each column vertically.
        CssRule::new(".wb-board-page")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("min-height", "100%"),
        CssRule::new(".wb-board")
            .property("display", "grid")
            .property("grid-auto-flow", "column")
            .property("grid-auto-columns", "minmax(16rem, 1fr)")
            .property("align-items", "start")
            .property("gap", "1rem")
            .property("width", "100%")
            .property("overflow-x", "auto")
            .property("padding-bottom", "0.5rem"),
        CssRule::new(".wb-column")
            .property("border", "1px solid var(--fui-border)")
            .property("border-radius", "var(--fui-radius-xl)")
            .property("background-color", "var(--fui-bg-sunken)")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.75rem")
            .property("padding", "0.75rem")
            .property("min-height", "12rem"),
        CssRule::new(".wb-column-title")
            .property("font-weight", "600")
            .property("font-size", "0.8125rem")
            .property("padding", "0.25rem 0.25rem 0")
            .property("color", "var(--fui-text-tertiary)"),
        // Fills the stretched column so even an empty one is a drop target.
        CssRule::new(".wb-column-body")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.625rem")
            .property("flex", "1 1 auto")
            .property("min-height", "6rem"),
        CssRule::new(".wb-column-body.wb-drop-target")
            .property("outline", "2px dashed var(--fui-brand-500)")
            .property("outline-offset", "-2px")
            .property("border-radius", "var(--fui-radius-lg)"),
        CssRule::new(".wb-card")
            .property("border", "1px solid var(--fui-border)")
            .property("border-radius", "var(--fui-radius-lg)")
            .property("background-color", "var(--fui-surface)")
            .property("box-shadow", "var(--fui-shadow-xs)")
            .property("padding", "0.75rem")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem"),
        // `.wb-card` is shared with the (non-draggable) comment list.
        CssRule::new(".wb-column-body .wb-card").property("cursor", "grab"),
        CssRule::new(".wb-card-key")
            .property("font-size", "0.75rem")
            .property("color", "var(--fui-text-tertiary)"),
        CssRule::new(".wb-card-title")
            .property("font-size", "0.9375rem")
            .property("font-weight", "500")
            .property("color", "var(--fui-text) !important")
            .property("text-decoration", "none"),
        CssRule::new(".wb-card-meta")
            .property("display", "flex")
            .property("flex-wrap", "wrap")
            .property("align-items", "center")
            .property("gap", "0.5rem")
            .property("font-size", "0.75rem")
            .property("color", "var(--fui-text-tertiary)"),
        // The inline status <select> on each card doubles as the no-drag (touch) way to move it.
        CssRule::new(".wb-card select").property("min-height", "2.25rem"),
        // Assignee select + "assign to me" shortcut share a row, wrapping on narrow screens.
        CssRule::new(".wb-field-control")
            .property("display", "flex")
            .property("flex-wrap", "wrap")
            .property("gap", "0.5rem")
            .property("min-width", "0"),
        CssRule::new(".wb-field-control select")
            .property("flex", "1 1 10rem")
            .property("min-width", "0"),
        CssRule::new(".wb-form-row")
            .property("display", "grid")
            .property(
                "grid-template-columns",
                "repeat(auto-fit, minmax(12rem, 1fr))",
            )
            .property("gap", "1rem"),
    ]
}

/// The issue detail page's dependency lists and the add-link form under them.
fn link_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".wb-link-section-title")
            .property("font-size", "0.8125rem")
            .property("font-weight", "600")
            .property("color", "var(--fui-text-tertiary)")
            .property("margin-bottom", "0.5rem"),
        CssRule::new(".wb-link-list")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem"),
        CssRule::new(".wb-link-row")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.75rem")
            .property("padding", "0.5rem 0.75rem")
            .property("border", "1px solid var(--fui-border)")
            .property("border-radius", "var(--fui-radius-md)")
            .property("background-color", "var(--fui-surface-2)"),
        CssRule::new(".wb-link-title")
            .property("flex", "1 1 auto")
            .property("min-width", "0")
            .property("overflow", "hidden")
            .property("text-overflow", "ellipsis")
            .property("white-space", "nowrap"),
        // Unwraps the form so the button itself, not its block parent, sizes.
        CssRule::new(".wb-link-remove-form").property("display", "contents"),
    ]
}
