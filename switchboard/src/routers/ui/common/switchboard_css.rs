use quench_srv::actix::routers::ui::common::css;
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
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules.extend(css::meta_rules());
    rules.extend(models_rules());
    rules.extend(estimates_modal_rules());
    rules
}

fn models_rules() -> Vec<CssRule> {
    vec![
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
    ]
}

fn estimates_modal_rules() -> Vec<CssRule> {
    vec![
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
