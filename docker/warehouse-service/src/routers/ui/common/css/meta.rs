use quench_web::prelude::CssRule;

pub fn meta_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".tag-link")
            .property("display", "flex")
            .property("align-items", "center")
            .property("padding", "0.25rem 0.5rem")
            .property("border-radius", "0.25rem")
            .property("text-decoration", "none")
            .property("color", "var(--bs-gray-400)")
            .property("font-size", "0.9rem")
            .property("transition", "background-color 0.2s, color 0.2s")
            .child(CssRule::new("i").property("width", "1.2rem").property("flex-shrink", "0"))
            .child(
                CssRule::new("&:hover")
                    .property("color", "var(--bs-gray-100)")
                    .property("background-color", "var(--bs-gray-800)"),
            ),
        CssRule::new(".tag-link.active")
            .property("background-color", "var(--bs-success-900)")
            .property("color", "var(--bs-gray-100)")
            .child(CssRule::new("i").property("color", "var(--bs-gray-100)")),
        CssRule::new(".meta-row")
            .property("display", "grid")
            .property("grid-template-columns", "10rem minmax(0, 1fr)")
            .property("min-width", "100%")
            .property("width", "max-content")
            .property(r"gap", "0.75rem")
            .property("padding", "0.35rem 0"),
        CssRule::new(".meta-label").property("color", "var(--bs-gray-400)"),
        CssRule::new(".mono").property("font-family", "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, \"Liberation Mono\", \"Courier New\", monospace"),
        // Dependency display within metadata panel
        CssRule::new(".meta-deps")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem"),
        CssRule::new(".deps-group")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.15rem"),
        CssRule::new(".deps-group-label")
            .property("font-size", "0.75rem")
            .property("text-transform", "uppercase")
            .property("letter-spacing", "0.05em")
            .property("color", "var(--bs-gray-500)")
            .property("margin-bottom", "0.2rem"),
        CssRule::new(".dep-row")
            .property("font-size", "0.85rem")
            .property("color", "var(--bs-gray-300)")
            .property("padding", "0.1rem 0"),
    ]
}
