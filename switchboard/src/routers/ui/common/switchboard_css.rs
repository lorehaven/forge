use quench_web::prelude::CssRule;

pub fn ensure_switchboard_css() {
    let css = switchboard_css_rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");

    let _ = std::fs::create_dir_all("dist/assets/css");
    let _ = std::fs::write("dist/assets/css/switchboard.css", css);
}

fn switchboard_css_rules() -> Vec<CssRule> {
    vec![
        CssRule::new("header").child(
            CssRule::new(".left-panel")
                .property("flex", "1")
                .property("min-width", "0")
                .property("justify-content", "left !important")
                .child(
                    CssRule::new("h2")
                        .property("margin", "0")
                        .property("white-space", "nowrap")
                        .property("overflow", "hidden")
                        .property("text-overflow", "ellipsis"),
                ),
        ),
        CssRule::new(".content")
            .property("overflow-y", "hidden")
            .property("padding", "1rem"),
        CssRule::new(".content-inner")
            .property("min-height", "unset")
            .property("width", "100%")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("justify-content", "flex-start")
            .property("align-items", "flex-start")
            .property("padding", "0"),
        CssRule::new(".page")
            .property("width", "100%")
            .property("flex", "1 1 auto")
            .child(
                CssRule::new(".page-header")
                    .property("height", "5rem")
                    .property("display", "flex")
                    .property("justify-content", "space-between")
                    .property("align-items", "center"),
            )
            .child(
                CssRule::new(".split-view")
                    .property("display", "grid")
                    .property(
                        "grid-template-columns",
                        "minmax(20rem, 28rem) minmax(0, 1fr)",
                    )
                    .property("gap", "1rem")
                    .property("height", "calc(100vh - 10rem)"),
            )
            .child(
                CssRule::new("@media screen and (max-width: 1024px)")
                    .child(CssRule::new(".split-view").property("grid-template-columns", "1fr")),
            ),
        CssRule::new("header .right-panel")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "1rem")
            .child(CssRule::new("a.button").property("padding", "0.6rem 1rem")),
        CssRule::new(".panel")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.3rem")
            .property("background-color", "var(--bs-gray-900)")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("min-height", "0")
            .property("overflow", "hidden"),
        CssRule::new(".panel-title")
            .property("padding", "0.75rem 1rem")
            .property("font-weight", "600")
            .property("border-bottom", "0.1rem solid var(--bs-gray-700)")
            .property("background-color", "var(--bs-gray-800)"),
        // Home / service index
        CssRule::new(".meta-list")
            .property("padding", "0.75rem 1rem")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem")
            .property("flex", "1 1 auto")
            .property("min-height", "0")
            .property("overflow", "auto"),
        CssRule::new(".home-content").property("width", "100%"),
        CssRule::new(".home-container")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "2rem")
            .property("max-width", "84rem")
            .property("margin", "0")
            .property("padding", "3rem"),
        CssRule::new("@media screen and (max-width: 768px)")
            .child(CssRule::new(".home-container").property("padding", "1.5rem")),
        CssRule::new(".home-header")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.4rem"),
        CssRule::new(".home-sections")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "2rem"),
        CssRule::new(".home-section")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.8rem"),
        CssRule::new(".home-section-title")
            .property("margin", "0")
            .property("font-size", "1rem")
            .property("font-weight", "700")
            .property("letter-spacing", "0.04em")
            .property("text-transform", "uppercase")
            .property("color", "var(--bs-gray-500)"),
        CssRule::new(".home-subtitle")
            .property("color", "var(--bs-gray-500)")
            .property("margin", "0"),
        CssRule::new(".home-grid")
            .property("display", "grid")
            .property(
                "grid-template-columns",
                "repeat(auto-fill, minmax(23rem, 1fr))",
            )
            .property("gap", "1.25rem"),
        CssRule::new(".home-card")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "space-between")
            .property("min-height", "8rem")
            .property("padding", "0 2rem")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.4rem")
            .property("background-color", "var(--bs-gray-900)")
            .property("text-decoration", "none")
            .property("color", "inherit")
            .property("transition", "border-color 0.15s, background-color 0.15s")
            .child(
                CssRule::new("&:hover")
                    .property("border-color", "var(--bs-gray-500)")
                    .property("background-color", "var(--bs-gray-800)"),
            ),
        CssRule::new(".home-card-body")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.55rem"),
        CssRule::new(".home-card-title")
            .property("font-size", "1.2rem")
            .property("font-weight", "600")
            .property("color", "var(--bs-gray-100)"),
        CssRule::new(".home-card-desc")
            .property("font-size", "0.95rem")
            .property("color", "var(--bs-gray-400)"),
        CssRule::new(".home-card-arrow")
            .property("font-size", "1.25rem")
            .property("color", "var(--bs-gray-500)")
            .property("flex-shrink", "0")
            .property("padding-left", "1rem"),
        // Models dashboard
        CssRule::new(".content:has(div > div.page > .models-dashboard-content)")
            .property("padding", "0")
            .property("height", "100%"),
        CssRule::new(".content-inner:has(div.page > .models-dashboard-content)")
            .property("box-sizing", "border-box")
            .property("padding", "0"),
        CssRule::new(".models-dashboard-content")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("height", "90vh")
            .property("overflow-y", "auto")
            .child(
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
                            .child(
                                CssRule::new("input")
                                    .property("width", "12rem")
                                    .property("padding", "0.38rem"),
                            )
                            .child(
                                CssRule::new("select").property("width", "8rem").child(
                                    CssRule::new("option.hidden").property("display", "none"),
                                ),
                            ),
                    )
                    .child(CssRule::new(".flex-1").property("flex", "1")),
            )
            .child(
                CssRule::new(".grid")
                    .property("flex", "1")
                    .property("padding", "1rem 0.5rem")
                    .property("display", "grid")
                    .property(
                        "grid-template-columns",
                        "repeat(auto-fill, minmax(520px, 1fr))",
                    )
                    .property("gap", "1rem")
                    .child(
                        CssRule::new(".card")
                            .property("display", "flex")
                            .property("flex-direction", "column")
                            .property("gap", "1rem")
                            .property("padding", "2rem 1.25rem")
                            .property("background", "var(--bs-gray-900)")
                            .child(
                                CssRule::new(".card-title")
                                    .property("font-family", "monospace")
                                    .property("font-size", "1.2rem")
                                    .property("font-weight", "bold")
                                    .property("word-break", "break-word"),
                            )
                            .child(
                                CssRule::new(".card-meta")
                                    .property("font-family", "monospace")
                                    .property("padding", "0.4rem")
                                    .property("display", "grid")
                                    .property(
                                        "grid-template-columns",
                                        "repeat(auto-fit, minmax(140px, 1fr))",
                                    )
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
                                    .property("cursor", "pointer")
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
                                        CssRule::new(".fit-separator").property("opacity", "0.5"),
                                    )
                                    .child(
                                        CssRule::new(".fit-ok")
                                            .property("background", "rgba(25, 135, 84, 0.18)")
                                            .property("border", "1px solid rgba(25, 135, 84, 0.5)")
                                            .property("color", "rgb(120, 255, 170)"),
                                    )
                                    .child(
                                        CssRule::new(".fit-warn")
                                            .property("background", "rgba(255, 193, 7, 0.18)")
                                            .property("border", "1px solid rgba(255, 193, 7, 0.5)")
                                            .property("color", "rgb(255, 230, 140)"),
                                    )
                                    .child(
                                        CssRule::new(".fit-no")
                                            .property("background", "rgba(220, 53, 69, 0.18)")
                                            .property("border", "1px solid rgba(220, 53, 69, 0.5)")
                                            .property("color", "rgb(255, 160, 170)"),
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
        // Login
        CssRule::new(".login-layout")
            .property("min-height", "calc(100vh - 10rem)")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "center"),
        CssRule::new(".login-panel")
            .property("width", "100%")
            .property("max-width", "28rem"),
        CssRule::new("#estimates-modal")
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
                    .property("border-radius", "0.5rem"),
            )
            .child(
                CssRule::new(".estimates-modal-header")
                    .property("display", "flex")
                    .property("justify-content", "space-between")
                    .property("align-items", "center")
                    .property("padding", "1rem 1.25rem")
                    .property("border-bottom", "1px solid var(--bs-gray-700)"),
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
                    .property("color", "white")
                    .property("font-size", "2rem")
                    .property("cursor", "pointer"),
            )
            .child(
                CssRule::new(".estimates-modal-body")
                    .property("display", "flex")
                    .property("flex-direction", "column")
                    .property("align-items", "center")
                    .property("padding", "1rem")
                    .property("overflow", "auto"),
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
                    )
                    .child(
                        CssRule::new(".fit-line.fit-ok")
                            .property("background-color", "#1e3a2f")
                            .property("border", "1px solid rgba(25, 135, 84, 0.5)")
                            .property("color", "#a1e3b8"),
                    )
                    .child(
                        CssRule::new(".fit-line.fit-warn")
                            .property("background-color", "#3f2e1e")
                            .property("border", "1px solid rgba(255, 193, 7, 0.5)")
                            .property("color", "#ffd38a"),
                    )
                    .child(
                        CssRule::new(".fit-line.fit-no")
                            .property("background-color", "#3a1f1f")
                            .property("border", "1px solid rgba(220, 53, 69, 0.5)")
                            .property("color", "#ff9a9a"),
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
                            .property("border", "1px solid var(--bs-gray-700)")
                            .property("color", "white")
                            .property("padding", "0.5rem")
                            .property("font-family", "monospace")
                            .property("font-size", "0.85rem"),
                    ),
            ),
    ]
}
