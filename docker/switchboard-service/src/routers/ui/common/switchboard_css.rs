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
    rules.extend(header_rules());
    rules.extend(shared_dashboard_rules());
    rules.extend(models_rules());
    rules.extend(estimates_modal_rules());
    rules.extend(vllm_rules());
    rules
}

fn shared_dashboard_rules() -> Vec<CssRule> {
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
                CssRule::new(".toolbar-action")
                    .property("display", "flex")
                    .property("align-items", "center")
                    .property("gap", "0.5rem")
                    .property("cursor", "pointer")
                    .property("font-weight", "500")),
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

fn vllm_rules() -> Vec<CssRule> {
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

fn estimates_modal_rules() -> Vec<CssRule> {
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
