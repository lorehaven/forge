use crate::styling::css::CssRule;

pub fn header() -> Vec<CssRule> {
    vec![
        CssRule::new("header,\n.q-shell-header")
            .property("background-color", "var(--q-shell-panel-bg-strong)")
            .property("height", "4rem")
            .property("display", "flex")
            .property("flex", "0 0 auto")
            .property("justify-content", "space-between")
            .property("align-items", "center")
            .property("padding", "0 1rem")
            .property("border-bottom", "var(--q-shell-panel-border)")
            .property("box-shadow", "var(--q-shell-panel-shadow)")
            .child(CssRule::new(".left-panel,\n.q-shell-header-left")
                .property("display", "flex")
                .property("justify-content", "center")
                .property("align-items", "center")
                .property("gap", "1rem")
                .child(CssRule::new("nav,\n.q-shell-nav-trigger")
                    .property("padding", "0.5rem")
                    .property("border-radius", "var(--q-shell-panel-radius)")
                    .property("border", "0.1rem solid var(--q-shell-text)")
                    .property("color", "var(--q-shell-text)")
                    .property("background-color", "var(--q-shell-panel-bg-strong)")
                    .property("cursor", "pointer")
                    .property("transition", "color 0.3s ease, border-color 0.3s ease, background-color 0.3s ease")
                    .child(CssRule::new("i")
                        .property("color", "unset")
                        .property("font-size", "1.6rem"))
                    .child(CssRule::new("&:hover")
                        .property("color", "var(--bs-gray-100)")
                        .property("border-color", "var(--bs-gray-100)")
                        .property("background-color", "var(--q-shell-panel-bg-soft)"))
                    .child(CssRule::new("&:active")
                        .property("color", "var(--bs-gray-100)")
                        .property("border-color", "var(--bs-gray-100)")
                        .property("background-color", "var(--bs-gray-700)"))))
    ]
}

pub fn content() -> Vec<CssRule> {
    vec![
        CssRule::new(".content,\n.q-shell-content")
            .property("flex", "1 1 auto")
            .property("overflow-x", "hidden")
            .property("overflow-y", "auto")
            .property("background", "var(--q-shell-panel-bg-soft)"),
        CssRule::new(".content-inner,\n.q-shell-content-inner")
            .property("min-height", "100%")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("justify-content", "center")
            .property("align-items", "center")
            .property("padding", "1rem"),
    ]
}

pub fn footer() -> Vec<CssRule> {
    vec![
        CssRule::new("footer,\n.q-shell-footer")
            .property("background-color", "var(--q-shell-panel-bg-strong)")
            .property("height", "3rem")
            .property("display", "flex")
            .property("flex", "0 0 auto")
            .property("justify-content", "center")
            .property("align-items", "center")
            .property("border-top", "var(--q-shell-panel-border)"),
    ]
}
