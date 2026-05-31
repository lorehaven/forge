use quench_web::prelude::CssRule;

pub fn utility_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".mr-2").property("margin-right", "0.5rem"),
        CssRule::new(".mt-4").property("margin-top", "1.5rem"),
        CssRule::new(".d-flex").property("display", "flex"),
        CssRule::new(".flex-column").property("flex-direction", "column"),
        CssRule::new(".h-100").property("height", "100%"),
        CssRule::new(".flex-1").property("flex", "1"),
        CssRule::new(".button-danger-sm")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("padding", "0.35rem 0.75rem")
            .property("background-color", "transparent")
            .property("color", "var(--bs-danger)")
            .property("border", "0.1rem solid var(--bs-danger)")
            .property("border-radius", "0.3rem")
            .property("font-size", "0.85rem")
            .property("font-weight", "500")
            .property("cursor", "pointer")
            .property("transition", "all 0.15s ease-in-out")
            .child(
                CssRule::new("&:hover")
                    .property("background-color", "var(--bs-danger)")
                    .property("color", "white"),
            )
            .child(
                CssRule::new("&:active")
                    .property("transform", "scale(0.96)")
                    .property("background-color", "var(--bs-danger-700)"),
            ),
    ]
}
