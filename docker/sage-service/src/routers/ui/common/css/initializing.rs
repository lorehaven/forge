use quench_web::prelude::CssRule;

pub fn initializing_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".init-content")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "center")
            .property("flex", "1")
            .property("width", "100%")
            .property("height", "100%")
            .property("padding", "2rem"),
        CssRule::new(".init-card")
            .property("background-color", "#1e1e1e")
            .property("border", "1px solid rgba(255, 255, 255, 0.1)")
            .property("border-radius", "12px")
            .property("box-shadow", "0 10px 30px rgba(0, 0, 0, 0.5)")
            .property("padding", "2.5rem")
            .property("width", "440px")
            .property("max-width", "90%")
            .property("text-align", "center"),
        CssRule::new(".init-spinner")
            .property("font-size", "2rem")
            .property("color", "var(--bs-primary)")
            .property("margin-bottom", "1rem"),
        CssRule::new(".init-title")
            .property("margin", "0 0 0.5rem")
            .property("font-size", "1.35rem")
            .property("color", "white"),
        CssRule::new(".init-subtitle")
            .property("margin", "0 0 1.75rem")
            .property("font-size", "0.9rem")
            .property("color", "rgba(255, 255, 255, 0.55)")
            .property("line-height", "1.4"),
        // Model rows
        CssRule::new(".model-rows")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem")
            .property("text-align", "left"),
        CssRule::new(".model-row")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "space-between")
            .property("gap", "1rem")
            .property("padding", "0.75rem 1rem")
            .property("background-color", "#252525")
            .property("border", "1px solid rgba(255, 255, 255, 0.06)")
            .property("border-radius", "8px"),
        CssRule::new(".model-row--running").property("border-color", "rgba(34, 197, 94, 0.35)"),
        CssRule::new(".model-row--failed").property("border-color", "rgba(239, 68, 68, 0.35)"),
        CssRule::new(".model-row-name")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.6rem")
            .property("font-size", "0.9rem")
            .property("color", "rgba(255, 255, 255, 0.9)")
            .property("overflow", "hidden")
            .property("text-overflow", "ellipsis")
            .property("white-space", "nowrap"),
        CssRule::new(".model-row-icon").property("color", "rgba(255, 255, 255, 0.35)"),
        CssRule::new(".model-status")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("gap", "0.4rem")
            .property("font-size", "0.8rem")
            .property("font-weight", "500")
            .property("white-space", "nowrap"),
        CssRule::new(".model-status--running").property("color", "#22c55e"),
        CssRule::new(".model-status--starting").property("color", "var(--bs-primary)"),
        CssRule::new(".model-status--pending").property("color", "rgba(255, 255, 255, 0.45)"),
        CssRule::new(".model-status--failed").property("color", "#ef4444"),
        CssRule::new(".model-status--unknown").property("color", "rgba(255, 255, 255, 0.45)"),
        CssRule::new(".init-warning")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "center")
            .property("gap", "0.5rem")
            .property("margin-top", "1.5rem")
            .property("font-size", "0.8rem")
            .property("color", "#eab308"),
    ]
}
