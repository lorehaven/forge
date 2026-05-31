use quench_web::prelude::CssRule;

pub fn header_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".header-split")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "1rem")
            .child(
                CssRule::new(".separator")
                    .property("color", "var(--bs-gray-600)")
                    .property("font-size", "1.5rem")
                    .property("font-weight", "300"),
            )
            .child(CssRule::new("h2").property("margin", "0")),
    ]
}
