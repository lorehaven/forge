use quench_srv::actix::routers::ui::common::css;
use quench_web::prelude::CssRule;

pub fn ensure_switchboard_css() {
    let css = switchboard_css_rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");

    let _ = std::fs::create_dir_all("../../../../../../dist/assets/css");
    let _ = std::fs::write("dist/assets/css/switchboard.css", css);
}

fn switchboard_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules.extend(css::meta_rules());
    rules.extend(header_rules());
    rules.extend(models_rules());
    rules.extend(estimates_modal_rules());
    rules
}

fn header_rules() -> Vec<CssRule> {
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
                    )
                    .child(CssRule::new(".flex-1").property("flex", "1")),
            )
            .child(
                CssRule::new(".grid")
                    .property("padding", "1rem 0.5rem")
                    .property("display", "grid")
                    .property(
                        "grid-template-columns",
                        "repeat(auto-fill, minmax(32.5rem, 1fr))",
                    )
                    .property("grid-template-rows", "max-content")
                    .property("gap", "1rem")
                    .child(
                        CssRule::new(".card")
                            .property("display", "flex")
                            .property("flex-direction", "column")
                            .property("gap", "1rem")
                            .property("padding", "2rem 1.25rem")
                            .property("background", "var(--bs-gray-900)")
                            .property("position", "relative")
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
                                            .property("transition", "color 0.2s"),
                                    )
                                    .child(
                                        CssRule::new(".card-delete:hover")
                                            .property("color", "var(--bs-danger)"),
                                    )
                                    .child(
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
                                            .property("background", "var(--bs-success-900)")
                                            .property("opacity", "0.8")
                                            .property(
                                                "border",
                                                "0.0625rem solid var(--bs-success-700)",
                                            )
                                            .property("color", "var(--bs-gray-100)"),
                                    )
                                    .child(
                                        CssRule::new(".fit-warn")
                                            .property("background", "var(--bs-warning)")
                                            .property("opacity", "0.8")
                                            .property("border", "0.0625rem solid var(--bs-warning)")
                                            .property("color", "var(--bs-gray-950)"),
                                    )
                                    .child(
                                        CssRule::new(".fit-no")
                                            .property("background", "var(--bs-danger)")
                                            .property("opacity", "0.8")
                                            .property("border", "0.0625rem solid var(--bs-danger)")
                                            .property("color", "var(--bs-gray-100)"),
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
        CssRule::new("#estimates-modal, #confirm-delete-modal")
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
                    )
                    .child(
                        CssRule::new(".fit-line.fit-ok")
                            .property("background-color", "var(--bs-success-900)")
                            .property("border", "0.0625rem solid var(--bs-success-700)")
                            .property("color", "var(--bs-gray-100)"),
                    )
                    .child(
                        CssRule::new(".fit-line.fit-warn")
                            .property("background-color", "var(--bs-warning)")
                            .property("border", "0.0625rem solid var(--bs-warning)")
                            .property("color", "var(--bs-gray-950)"),
                    )
                    .child(
                        CssRule::new(".fit-line.fit-no")
                            .property("background-color", "var(--bs-danger)")
                            .property("border", "0.0625rem solid var(--bs-danger)")
                            .property("color", "var(--bs-gray-100)"),
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
