use quench_web::prelude::CssRule;

pub fn shared_dashboard_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".content:has(.models-dashboard-content), .content:has(.vllm-manage-content)")
            .property("padding", "0")
            .property("height", "100%"),
        CssRule::new(".content-inner:has(.models-dashboard-content), .content-inner:has(.vllm-manage-content)")
            .property("box-sizing", "border-box")
            .property("padding", "0"),
        CssRule::new(".top-bar")
            .property("display", "flex")
            .property("justify-content", "space-between")
            .property("align-items", "center")
            .property("margin", "0 0.5rem")
            .property("padding", "0.25rem 1rem")
            .property("background-color", "var(--bs-gray-900)")
            .property("border-radius", "0.1rem")
            .property("gap", "3rem")
            .property("position", "sticky")
            .property("top", "0")
            .property("z-index", "100")
            .child(
                CssRule::new(".gpu")
                    .property("display", "flex")
                    .property("gap", "0.3rem")
                    .child(
                        CssRule::new("div")
                            .property("padding", "0.5rem")
                            .property("background-color", "var(--bs-gray-800)"),
                    ),
            )
            .child(
                CssRule::new(".flex-1")
                    .property("flex", "1"))
            .child(
                CssRule::new("form")
                    .property("display", "flex")
                    .property("flex-direction", "row")
                    .property("gap", "2rem")
                    .property("width", "unset")
                    .property("margin", "auto")
            )
            .child(
                CssRule::new(".toolbar-action")
                    .property("display", "flex")
                    .property("align-items", "center")
                    .property("gap", "0.5rem")
                    .property("cursor", "pointer")
                    .property("font-weight", "500")
                    .property("text-decoration", "none")),
        CssRule::new(".grid")
            .property("padding", "1rem 0.5rem")
            .property("display", "grid")
            .property(
                "grid-template-columns",
                "repeat(auto-fill, minmax(32.5rem, 1fr))",
            )
            .property("grid-template-rows", "max-content")
            .property("gap", "1rem"),
        CssRule::new(".card")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "1rem")
            .property("padding", "2rem 1.25rem")
            .property("background", "var(--bs-gray-900)")
            .property("position", "relative")
            .property("user-select", "text")
            .child(
                CssRule::new(".card-header")
                    .property("display", "flex")
                    .property("justify-content", "space-between")
                    .property("align-items", "center")
                    .property("gap", "1rem")
                    .property("min-width", "0")
                    .child(
                        CssRule::new(".card-title")
                            .property("font-family", "monospace")
                            .property("font-size", "1.2rem")
                            .property("font-weight", "bold")
                            .property("overflow", "hidden")
                            .property("text-overflow", "ellipsis")
                            .property("white-space", "nowrap")
                            .property("min-width", "0")
                            .property("flex", "1"),
                    )
                    .child(
                        CssRule::new(".card-delete")
                            .property("flex-shrink", "0")
                            .property("background", "transparent")
                            .property("border", "none")
                            .property("color", "var(--bs-gray-600)")
                            .property("font-size", "1.1rem")
                            .property("line-height", "1")
                            .property("cursor", "pointer")
                            .property("padding", "0.2rem 0.5rem")
                            .property("transition", "color 0.2s")
                            .child(CssRule::new(":hover").property("color", "var(--bs-danger)")),
                    ),
            )
            .child(
                CssRule::new(".card-meta")
                    .property("font-family", "monospace")
                    .property("padding", "0.4rem")
                    .property("display", "grid")
                    .property("grid-template-columns", "1fr 1fr")
                    .property("gap", "0.75rem")
                    .property("font-size", "0.8rem")
                    .property("line-height", "1.5")
                    .property("background-color", "var(--bs-gray-950)"),
            )
            .child(
                CssRule::new(".card-fit")
                    .property("display", "flex")
                    .property("flex-direction", "column")
                    .property("gap", "0.5rem")
                    .property("cursor", "pointer"),
            )
            .child(
                CssRule::new(".fit-line")
                    .property("display", "flex")
                    .property("align-items", "center")
                    .property("gap", "0.5rem")
                    .property("padding", "0.65rem 0.75rem")
                    .property("border-radius", "0.4rem")
                    .property("font-family", "monospace")
                    .property("font-size", "0.8rem")
                    .property("line-height", "1")
                    .property("overflow-x", "auto")
                    .property("white-space", "nowrap"),
            )
            .child(
                CssRule::new(".fit-details-icon")
                    .property("margin-left", "auto")
                    .property("opacity", "0.75")
                    .property("flex-shrink", "0"),
            )
            .child(CssRule::new(".fit-separator").property("opacity", "0.5")),
        CssRule::new(".fit-ok")
            .property("background", "var(--bs-success-900)")
            .property("opacity", "0.8")
            .property("border", "0.0625rem solid var(--bs-success-700)")
            .property("color", "var(--bs-gray-100)"),
        CssRule::new(".fit-warn")
            .property("background", "var(--bs-warning)")
            .property("opacity", "0.8")
            .property("border", "0.0625rem solid var(--bs-warning)")
            .child(CssRule::new("*")
                .property("color", "var(--bs-gray-950)"))
            .child(CssRule::new(".badge")
                .property("background", "var(--bs-gray-300)")),
        CssRule::new(".fit-no")
            .property("background", "var(--bs-danger)")
            .property("opacity", "0.8")
            .property("border", "0.0625rem solid var(--bs-danger)")
            .property("color", "var(--bs-gray-100)"),
    ]
}
