use quench_web::prelude::CssRule;

pub fn models_rules() -> Vec<CssRule> {
    vec![
        // Models dashboard
        CssRule::new(".models-dashboard-content")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("height", "90vh")
            .property("overflow-y", "auto")
            .child(
                CssRule::new(".top-bar")
                    .child(
                        CssRule::new(".model-tabs")
                            .property("display", "flex")
                            .property("gap", "0.3rem")
                            .child(
                                CssRule::new(".tab")
                                    .property("cursor", "pointer")
                                    .property("width", "3rem")
                                    .property("padding", "0.5rem")
                                    .property("text-align", "center")
                                    .property("background-color", "var(--bs-gray-800)"),
                            )
                            .child(
                                CssRule::new(".tab:hover")
                                    .property("background-color", "var(--bs-gray-700)"),
                            )
                            .child(
                                CssRule::new(".tab.active")
                                    .property("background-color", "var(--bs-gray-600)"),
                            ),
                    )
                    .child(
                        CssRule::new(".model-filters")
                            .property("display", "flex")
                            .property("gap", "0.2rem")
                            .child(
                                CssRule::new("input")
                                    .property("width", "12rem")
                                    .property("padding", "0.38rem"),
                            )
                            .child(
                                CssRule::new("select").property("width", "8rem").child(
                                    CssRule::new("option.hidden").property("display", "none"),
                                ),
                            )
                            .child(CssRule::new("#sort").property("width", "9rem"))
                            .child(
                                CssRule::new(".vllm-filter")
                                    .property("display", "flex")
                                    .property("align-items", "center")
                                    .property("gap", "0.4rem")
                                    .property("color", "var(--q-shell-text)")
                                    .property("font-size", "0.9rem")
                                    .property("cursor", "pointer")
                                    .property("user-select", "none")
                                    .property("margin-left", "0.5rem")
                                    .property("border", "0.1rem solid var(--bs-gray-700)")
                                    .property("border-radius", "0.3rem")
                                    .property("padding", "0.5rem 1rem")
                                    .child(
                                        CssRule::new("input")
                                            .property("width", "unset")
                                            .property("cursor", "pointer")
                                            .property("margin", "0")
                                            .property("accent-color", "var(--bs-success-500)"),
                                    ),
                            ),
                    ),
            )
            .child(
                CssRule::new(".grid").child(
                    CssRule::new(".card")
                        .child(
                            CssRule::new(".card-header").child(
                                CssRule::new(".card-title").child(
                                    CssRule::new(".vllm-badge")
                                        .property("background-color", "var(--bs-success-500)")
                                        .property("color", "var(--bs-gray-950)")
                                        .property("font-size", "0.6rem")
                                        .property("padding", "0.15rem 0.5rem")
                                        .property("border-radius", "1rem") // Pill shape
                                        .property("margin-right", "0.5rem")
                                        .property("vertical-align", "middle")
                                        .property("font-family", "sans-serif")
                                        .property("font-weight", "800")
                                        .property("text-transform", "uppercase")
                                        .property("letter-spacing", "0.05rem")
                                        .property("display", "inline-block")
                                        .property(
                                            "box-shadow",
                                            "0 0.0625rem 0.125rem rgba(0,0,0,0.1)",
                                        ),
                                ),
                            ),
                        )
                        .child(
                            CssRule::new(".card-path")
                                .property("font-family", "monospace")
                                .property("font-size", "0.7rem")
                                .property("opacity", "0.7")
                                .property("word-break", "break-all"),
                        ),
                ),
            ),
    ]
}
