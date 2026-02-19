use crate::styling::css::CssRule;

pub fn header() -> Vec<CssRule> {
    vec![
        CssRule::new("header")
            .property("background-color", "var(--neutral-950)")
            .property("height", "4rem")
            .property("display", "flex")
            .property("flex", "0 0 auto")
            .property("justify-content", "space-between")
            .property("align-items", "center")
            .property("padding", "0 1rem")
            .child(CssRule::new(".left-panel")
                .property("display", "flex")
                .property("justify-content", "center")
                .property("align-items", "center")
                .property("gap", "1rem")
                .child(CssRule::new("nav")
                    .property("padding", "0.5rem")
                    .property("border-radius", "0.25rem")
                    .property("border", "0.1rem solid var(--neutral-300)")
                    .property("color", "var(--neutral-300)")
                    .property("background-color", "var(--neutral-950)")
                    .property("cursor", "pointer")
                    .property("transition", "color 0.3s ease, border-color 0.3s ease, background-color 0.3s ease")
                    .child(CssRule::new("i")
                        .property("color", "unset")
                        .property("font-size", "1.6rem"))
                    .child(CssRule::new("&:hover")
                        .property("color", "var(--neutral-100)")
                        .property("border-color", "var(--neutral-100)")
                        .property("background-color", "var(--neutral-800)"))
                    .child(CssRule::new("&:active")
                        .property("color", "var(--neutral-100)")
                        .property("border-color", "var(--neutral-100)")
                        .property("background-color", "var(--neutral-700)"))))
    ]
}

pub fn content() -> Vec<CssRule> {
    vec![
        CssRule::new("content")
            .property("flex", "1 1 auto")
            .property("overflow-x", "hidden")
            .property("overflow-y", "auto"),
    ]
}

pub fn footer() -> Vec<CssRule> {
    vec![
        CssRule::new("footer")
            .property("background-color", "var(--neutral-950)")
            .property("height", "3rem")
            .property("display", "flex")
            .property("flex", "0 0 auto")
            .property("justify-content", "center")
            .property("align-items", "center"),
    ]
}
