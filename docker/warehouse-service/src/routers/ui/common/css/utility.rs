use quench_web::prelude::CssRule;

pub fn utility_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".mr-2").property("margin-right", "0.5rem"),
        CssRule::new(".mt-4").property("margin-top", "1.5rem"),
        CssRule::new(".d-flex").property("display", "flex"),
        CssRule::new(".flex-column").property("flex-direction", "column"),
        CssRule::new(".h-100").property("height", "100%"),
        CssRule::new(".flex-1").property("flex", "1"),
        CssRule::new(".inline-action-form")
            .property("display", "inline-flex")
            .property("width", "fit-content")
            .property("align-self", "flex-start"),
        CssRule::new(".button-danger-sm")
            .property("display", "inline-flex")
            .property("width", "fit-content")
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
        CssRule::new("#confirm-delete-image-modal")
            .property("position", "fixed")
            .property("inset", "0")
            .property("display", "none")
            .property("z-index", "9999")
            .child(CssRule::new("&.open").property("display", "flex"))
            .child(
                CssRule::new(".confirm-modal-backdrop")
                    .property("position", "absolute")
                    .property("inset", "0")
                    .property("background", "rgba(0,0,0,0.7)"),
            )
            .child(
                CssRule::new(".confirm-modal-content")
                    .property("position", "relative")
                    .property("margin", "auto")
                    .property("width", "min(30rem, calc(100% - 2rem))")
                    .property("background", "var(--bs-gray-900)")
                    .property("border", "0.0625rem solid var(--bs-gray-700)")
                    .property("border-radius", "0.5rem")
                    .property("overflow", "hidden"),
            )
            .child(
                CssRule::new(".confirm-modal-header")
                    .property("display", "flex")
                    .property("justify-content", "space-between")
                    .property("align-items", "center")
                    .property("padding", "1rem 1.25rem")
                    .property("border-bottom", "0.0625rem solid var(--bs-gray-700)"),
            )
            .child(
                CssRule::new(".confirm-modal-title")
                    .property("font-size", "1.25rem")
                    .property("font-weight", "700")
                    .property("font-family", "monospace"),
            )
            .child(
                CssRule::new(".confirm-modal-close")
                    .property("border", "none")
                    .property("background", "transparent")
                    .property("color", "var(--bs-gray-100)")
                    .property("font-size", "1.3rem")
                    .property("cursor", "pointer")
                    .property("padding", "0.35rem")
                    .property("line-height", "1"),
            )
            .child(
                CssRule::new(".confirm-modal-body")
                    .property("display", "flex")
                    .property("flex-direction", "column")
                    .property("align-items", "center")
                    .property("padding", "1rem")
                    .child(
                        CssRule::new("p")
                            .property("text-align", "center")
                            .property("margin", "0 0 1rem 0"),
                    )
                    .child(
                        CssRule::new(".confirm-delete-target")
                            .property("font-family", "monospace")
                            .property("font-weight", "700")
                            .property("color", "var(--bs-warning)")
                            .property("margin-bottom", "1.5rem")
                            .property("word-break", "break-all")
                            .property("text-align", "center"),
                    )
                    .child(
                        CssRule::new(".confirm-actions")
                            .property("display", "flex")
                            .property("flex-direction", "row")
                            .property("flex-wrap", "nowrap")
                            .property("gap", "1rem")
                            .property("justify-content", "center")
                            .property("align-items", "center")
                            .property("width", "100%")
                            .child(
                                CssRule::new(".button")
                                    .property("min-width", "5.5rem")
                                    .property("width", "auto")
                                    .property("padding", "0.4rem 0.75rem")
                                    .property("font-size", "0.85rem")
                                    .property("line-height", "1.1"),
                            ),
                    ),
            ),
    ]
}
