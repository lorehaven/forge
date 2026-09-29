//! Gatehouse's stylesheet, built from the same rule sets the other services use.

use quench_starter::actix::routers::ui::common::css;
use quench_web::prelude::CssRule;

pub fn ensure_gatehouse_css() {
    forge_ui::write_css("gatehouse.css", &gatehouse_css_rules());
}

/// Shared sets plus the admin pages' own rows - just the user list/permission
/// matrix layout, since `style.css` already covers forms estate-wide.
pub fn gatehouse_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(css::meta_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules.extend(forge_ui::css_rules());
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
        // One row per user: name/roles left, grants middle, edit link pinned right.
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
        // Status rows: label, value, optional action button - same shape as `.admin-row`.
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
        // Label column fixed, action checkboxes wrap instead of overflowing.
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
        // Label left, control right, on every account form. One height for every
        // control so text inputs, selects, the file picker and checkboxes line up.
        CssRule::new(".form-row")
            .property("display", "flex")
            .property("justify-content", "space-between")
            .property("align-items", "center")
            .property("gap", "1rem")
            .property("margin", "0.4rem 0")
            .property("min-height", "3.5rem")
            .child(CssRule::new("label").property("margin", "0")),
        CssRule::new(".form-row > input,\n.form-row > select,\n.form-row > .file-picker")
            .property("flex", "0 1 60%")
            .property("width", "60%")
            .property("height", "3.5rem")
            .property("box-sizing", "border-box")
            .property("margin", "0"),
        CssRule::new(".form-row > select")
            .property("background-color", "var(--bs-gray-800)")
            .property("padding", "0.8rem")
            .property("font-size", "1.2rem"),
        CssRule::new(".form-row > input[type=checkbox]")
            .property("flex", "0 0 auto")
            .property("width", "1.4rem")
            .property("height", "1.4rem")
            .property("padding", "0")
            .property("accent-color", "var(--bs-success-700)"),
        CssRule::new(".file-picker")
            .property("display", "flex")
            .property("position", "relative")
            .child(
                CssRule::new(".file-picker-native")
                    .property("position", "absolute")
                    .property("opacity", "0")
                    .property("width", "1px")
                    .property("height", "1px")
                    .property("pointer-events", "none"),
            )
            .child(
                CssRule::new("label.file-picker-button")
                    .property("display", "flex")
                    .property("align-items", "center")
                    .property("gap", "0.6rem")
                    .property("flex", "0 0 auto")
                    .property("padding", "0 1rem")
                    .property("font-size", "1.2rem")
                    .property("cursor", "pointer")
                    .property("color", "var(--bs-gray-300)")
                    .property("background-color", "var(--bs-success-900)")
                    .property("border-radius", "0.3rem 0 0 0.3rem")
                    .property("transition", "background-color 0.3s ease")
                    .child(
                        CssRule::new("&:hover")
                            .property("background-color", "var(--bs-success-800)"),
                    ),
            )
            .child(
                CssRule::new("input.file-picker-name")
                    .property("flex", "1 1 auto")
                    .property("min-width", "0")
                    .property("height", "100%")
                    .property("box-sizing", "border-box")
                    .property("border-radius", "0 0.3rem 0.3rem 0")
                    .property("cursor", "default"),
            ),
        CssRule::new("img.account-avatar")
            .property("border-radius", "50%")
            .property("object-fit", "cover")
            .property("width", "4rem")
            .property("height", "4rem"),
        CssRule::new(".mfa-qr")
            .property("display", "flex")
            .property("justify-content", "center")
            .property("margin", "0.5rem 0")
            .child(
                CssRule::new("img")
                    .property("background", "#fff")
                    .property("padding", "0.5rem")
                    .property("border-radius", "0.3rem"),
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
