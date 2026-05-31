use quench_web::prelude::CssRule;

pub fn vllm_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".vllm-manage-content")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("height", "90vh")
            .property("overflow-y", "auto"),
        CssRule::new(".status-running")
            .property("color", "var(--bs-success-500)")
            .property("font-weight", "bold"),
        CssRule::new(".status-starting")
            .property("color", "var(--bs-warning)")
            .property("font-weight", "bold"),
        CssRule::new(".status-failed")
            .property("color", "var(--bs-danger)")
            .property("font-weight", "bold"),
        CssRule::new(".badge")
            .property("background", "var(--bs-gray-800)")
            .property("border", "0.0625rem solid var(--bs-gray-700)")
            .property("padding", "0.1rem 0.4rem")
            .property("border-radius", "0.3rem")
            .property("font-size", "0.75rem"),
        CssRule::new(".modal")
            .property("display", "none")
            .property("position", "fixed")
            .property("top", "0")
            .property("left", "0")
            .property("width", "100%")
            .property("height", "100%")
            .property("background", "rgba(0, 0, 0, 0.7)")
            .property("z-index", "1000")
            .property("justify-content", "center")
            .property("align-items", "center"),
        CssRule::new(".modal-content")
            .property("background", "var(--bs-gray-900)")
            .property("border", "0.0625rem solid var(--bs-gray-700)")
            .property("border-radius", "0.5rem")
            .property("width", "90%")
            .property("max-width", "40rem")
            .property("max-height", "90vh")
            .property("display", "flex")
            .property("flex-direction", "column"),
        CssRule::new(".modal-header")
            .property("padding", "1rem")
            .property("border-bottom", "0.0625rem solid var(--bs-gray-700)")
            .property("display", "flex")
            .property("justify-content", "space-between")
            .property("align-items", "center")
            .child(CssRule::new("h3").property("margin", "0")),
        CssRule::new(".modal-close")
            .property("border", "none")
            .property("background", "transparent")
            .property("color", "var(--bs-gray-100)")
            .property("font-size", "1.4rem")
            .property("cursor", "pointer")
            .property("padding", "0.5rem")
            .property("line-height", "1")
            .child(CssRule::new(":hover").property("color", "var(--bs-gray-400)")),
        CssRule::new(".modal-body")
            .property("padding", "1rem")
            .property("overflow-y", "auto")
            .property("flex", "1"),
        CssRule::new(".modal-footer")
            .property("padding", "1rem")
            .property("border-top", "0.0625rem solid var(--bs-gray-700)")
            .property("display", "flex")
            .property("justify-content", "flex-end")
            .property("gap", "1rem"),
        CssRule::new(".form-group")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.4rem")
            .property("margin-bottom", "1rem")
            .property("flex", "1"),
        CssRule::new(".form-row")
            .property("display", "flex")
            .property("gap", "1rem"),
        CssRule::new(".form-group-checkbox")
            .property("flex", "1")
            .child(CssRule::new("input").property("width", "unset")),
        CssRule::new(".fit-note")
            .property("margin-top", "0.5rem")
            .property("padding", "0.5rem")
            .property("border-radius", "0.4rem")
            .property("font-size", "0.9rem")
            .property("line-height", "1.4"),
        CssRule::new(".instance-diagnostics")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem")
            .property("font-family", "monospace")
            .property("font-size", "0.78rem")
            .property("line-height", "1.45")
            .child(
                CssRule::new(".instance-error")
                    .property("padding", "0.75rem")
                    .property("background", "var(--bs-gray-950)")
                    .property("border-left", "0.2rem solid var(--bs-danger)")
                    .property("white-space", "pre-wrap")
                    .property("word-break", "break-word"),
            )
            .child(
                CssRule::new(".instance-log-path")
                    .property("color", "var(--bs-gray-400)")
                    .property("word-break", "break-all"),
            ),
        CssRule::new(".launch-modal-content")
            .property("max-width", "48rem")
            .property("border-radius", "0.9rem")
            .property(
                "background",
                "linear-gradient(180deg, var(--bs-gray-900) 0%, var(--bs-gray-950) 100%)",
            )
            .property("box-shadow", "0 1.5rem 4rem rgba(0, 0, 0, 0.35)"),
        CssRule::new(".launch-modal .modal-header")
            .property("padding", "1.25rem 1.5rem")
            .property("align-items", "flex-start"),
        CssRule::new(".launch-modal-heading")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.45rem")
            .child(CssRule::new("h3").property("margin", "0"))
            .child(
                CssRule::new(".launch-modal-subtitle")
                    .property("margin", "0")
                    .property("color", "var(--bs-gray-400)")
                    .property("font-size", "0.92rem")
                    .property("line-height", "1.4"),
            ),
        CssRule::new(".launch-modal .modal-body").property("padding", "1.5rem"),
        CssRule::new(".launch-form-row").property("align-items", "stretch"),
        CssRule::new(".launch-modal .form-group label, .launch-modal .form-label")
            .property("font-size", "0.78rem")
            .property("font-weight", "700")
            .property("letter-spacing", "0.04rem")
            .property("text-transform", "uppercase")
            .property("color", "var(--bs-gray-400)"),
        CssRule::new(".launch-modal input, .launch-modal select")
            .property("box-sizing", "border-box")
            .property("min-height", "2.75rem")
            .property("padding", "0.7rem 0.85rem")
            .property("border", "0.0625rem solid var(--bs-gray-700)")
            .property("border-radius", "0.55rem")
            .property("background", "var(--bs-gray-950)")
            .property("color", "var(--bs-gray-100)"),
        CssRule::new(".launch-modal input:focus, .launch-modal select:focus")
            .property("outline", "none")
            .property("border-color", "var(--bs-success-500)")
            .property("box-shadow", "0 0 0 0.18rem rgba(25, 135, 84, 0.18)"),
        CssRule::new(".checkbox-control")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.75rem")
            .property("min-height", "2.75rem")
            .property("padding", "0.7rem 0.85rem")
            .property("border", "0.0625rem solid var(--bs-gray-700)")
            .property("border-radius", "0.55rem")
            .property("background", "var(--bs-gray-950)")
            .property("cursor", "pointer")
            .child(
                CssRule::new("input")
                    .property("margin", "0")
                    .property("accent-color", "var(--bs-success-500)")
                    .property("transform", "scale(1.05)"),
            ),
        CssRule::new(".checkbox-copy")
            .property("font-size", "0.9rem")
            .property("line-height", "1.35")
            .property("color", "var(--bs-gray-200)"),
        CssRule::new(".launch-modal .fit-note")
            .property("margin-top", "0.25rem")
            .property("padding", "0")
            .property("background", "transparent"),
        CssRule::new(".launch-modal .fit-note > div")
            .property("padding", "0.85rem 1rem")
            .property("border-radius", "0.55rem")
            .property("font-family", "monospace")
            .property("line-height", "1.4")
            .property("display", "flex")
            .property("align-items", "flex-start")
            .property("gap", "0.75rem")
            .child(CssRule::new("i").property("font-size", "1.2rem")),
        CssRule::new(".launch-modal .modal-footer").property("padding", "1rem 1.5rem 1.5rem"),
        CssRule::new("@media (max-width: 900px)").child(
            CssRule::new(".launch-form-row")
                .property("flex-direction", "column")
                .property("gap", "0"),
        ),
    ]
}
