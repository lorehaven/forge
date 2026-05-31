use quench_web::prelude::CssRule;

pub fn estimates_modal_rules() -> Vec<CssRule> {
    vec![
        CssRule::new("#estimates-modal, #confirm-delete-modal, #confirm-stop-instance-modal")
            .property("position", "fixed")
            .property("inset", "0")
            .property("display", "none")
            .property("z-index", "9999")
            .child(CssRule::new("&.open").property("display", "flex"))
            .child(
                CssRule::new(".estimates-modal-backdrop")
                    .property("position", "absolute")
                    .property("inset", "0")
                    .property("background", "rgba(0,0,0,0.7)"),
            )
            .child(
                CssRule::new(".estimates-modal-content")
                    .property("position", "relative")
                    .property("margin", "auto")
                    .property("width", "60rem")
                    .property("height", "90vh")
                    .property("overflow", "hidden")
                    .property("display", "flex")
                    .property("flex-direction", "column")
                    .property("background", "var(--bs-gray-900)")
                    .property("border-radius", "0.5rem")
                    .child(
                        CssRule::new("&.small")
                            .property("width", "30rem")
                            .property("height", "auto")
                            .property("max-height", "80vh"),
                    ),
            )
            .child(
                CssRule::new(".estimates-modal-header")
                    .property("display", "flex")
                    .property("justify-content", "space-between")
                    .property("align-items", "center")
                    .property("padding", "1rem 1.25rem")
                    .property("border-bottom", "0.0625rem solid var(--bs-gray-700)"),
            )
            .child(
                CssRule::new(".estimates-modal-title")
                    .property("font-size", "1.4rem")
                    .property("font-weight", "bold")
                    .property("font-family", "monospace"),
            )
            .child(
                CssRule::new(".estimates-modal-close")
                    .property("border", "none")
                    .property("background", "transparent")
                    .property("color", "var(--bs-gray-100)")
                    .property("font-size", "1.4rem")
                    .property("cursor", "pointer")
                    .property("padding", "0.5rem")
                    .property("line-height", "1")
                    .child(CssRule::new(":hover").property("color", "var(--bs-gray-400)")),
            )
            .child(
                CssRule::new(".estimates-modal-body")
                    .property("display", "flex")
                    .property("flex-direction", "column")
                    .property("align-items", "center")
                    .property("padding", "1rem")
                    .property("overflow", "auto")
                    .child(
                        CssRule::new("p")
                            .property("text-align", "center")
                            .property("margin-bottom", "1rem"),
                    )
                    .child(
                        CssRule::new(".model-to-delete-name")
                            .property("font-family", "monospace")
                            .property("font-weight", "bold")
                            .property("color", "var(--bs-warning)")
                            .property("margin-bottom", "2rem")
                            .property("word-break", "break-all")
                            .property("text-align", "center"),
                    )
                    .child(
                        CssRule::new(".confirm-actions")
                            .property("display", "flex")
                            .property("gap", "1rem")
                            .property("justify-content", "center")
                            .property("width", "100%"),
                    ),
            )
            .child(
                CssRule::new(".estimate-grid")
                    .property("display", "flex")
                    .property("flex-direction", "column")
                    .property("font-size", "1rem")
                    .property("gap", "0.2rem")
                    .child(
                        CssRule::new(".fit-line")
                            .property("font-size", "1.2rem")
                            .property("display", "grid")
                            .property("grid-template-columns", "repeat(4, 12rem)")
                            .property("gap", "0.4rem")
                            .property("align-items", "center")
                            .property("justify-content", "start")
                            .property("width", "fit-content")
                            .property("max-width", "100%")
                            .property("padding", "0.3rem")
                            .property("border-radius", "0.4rem"),
                    ),
            )
            .child(
                CssRule::new(".estimate-filters")
                    .property("display", "flex")
                    .property("gap", "0.75rem")
                    .property("margin-bottom", "1rem")
                    .property("flex-wrap", "wrap")
                    .child(
                        CssRule::new("select")
                            .property("background", "var(--bs-gray-800)")
                            .property("border", "0.0625rem solid var(--bs-gray-700)")
                            .property("color", "var(--bs-gray-100)")
                            .property("padding", "0.5rem")
                            .property("font-family", "monospace")
                            .property("font-size", "0.85rem"),
                    ),
            ),
    ]
}
