use quench_web::prelude::CssRule;

pub fn projects_rules() -> Vec<CssRule> {
    vec![
        // Projects & Context Separation
        CssRule::new(".history-section-header")
            .property("display", "flex")
            .property("justify-content", "space-between")
            .property("align-items", "center")
            .property("padding", "1rem 0.75rem 0.5rem")
            .property("margin", "0 0.25rem") // Add side margin
            .property("font-size", "0.75rem")
            .property("font-weight", "600")
            .property("text-transform", "uppercase")
            .property("color", "var(--bs-gray-500)")
            .property("letter-spacing", "0.05em"),
        CssRule::new(".add-project-btn")
            .property("border", "1px solid transparent")
            .property("background-color", "transparent")
            .property("color", "var(--bs-gray-400)")
            .property("border-radius", "0.375rem")
            .property("padding", "0.15rem 0.35rem")
            .property("cursor", "pointer")
            .property("font-size", "0.7rem")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("justify-content", "center")
            .property("gap", "0.3rem")
            .property("transition", "all 0.15s ease"),
        CssRule::new(".add-project-btn:hover")
            .property("background-color", "rgba(255, 255, 255, 0.05)")
            .property("color", "var(--bs-gray-200)")
            .property("border-color", "var(--bs-gray-700)"),
        CssRule::new(".project-item").property("margin-bottom", "2px"),
        CssRule::new(".collapsible")
            .property("cursor", "pointer")
            .property("user-select", "none"),
        CssRule::new(".collapsible:hover").property("color", "white"),
        CssRule::new(".collapsible .chevron")
            .property("transition", "transform 0.2s ease")
            .property("font-size", "0.6rem")
            .property("color", "rgba(255, 255, 255, 0.4)"),
        CssRule::new(".collapsible.open .chevron").property("transform", "rotate(90deg)"),
        CssRule::new(".history-section-content").property("display", "block"),
        CssRule::new(".history-section-content.hidden").property("display", "none"),
        CssRule::new(".project-conv-item")
            .property("margin-left", "1.5rem")
            .property("position", "relative"),
        CssRule::new(".project-conv-item::before")
            .property("content", "''")
            .property("position", "absolute")
            .property("left", "-0.75rem")
            .property("top", "-1rem")
            .property("bottom", "50%")
            .property("width", "0.5rem")
            .property("border-left", "1px solid rgba(255, 255, 255, 0.1)")
            .property("border-bottom", "1px solid rgba(255, 255, 255, 0.1)")
            .property("border-bottom-left-radius", "4px"),
        // Modals
        CssRule::new(".modal-backdrop")
            .property("position", "fixed")
            .property("top", "0")
            .property("left", "0")
            .property("width", "100%")
            .property("height", "100%")
            .property("background-color", "rgba(0, 0, 0, 0.7)")
            .property("display", "flex")
            .property("justify-content", "center")
            .property("align-items", "center")
            .property("z-index", "1000"),
        CssRule::new(".modal-content")
            .property("background-color", "#1e1e1e")
            .property("padding", "2rem")
            .property("border-radius", "8px")
            .property("width", "400px")
            .property("max-width", "90%")
            .property("border", "1px solid rgba(255, 255, 255, 0.1)")
            .property("box-shadow", "0 10px 25px rgba(0, 0, 0, 0.5)"),
        CssRule::new(".modal-content h2")
            .property("margin-top", "0")
            .property("margin-bottom", "1.5rem")
            .property("font-size", "1.25rem"),
        CssRule::new(".form-group").property("margin-bottom", "1.5rem"),
        CssRule::new(".form-group label")
            .property("display", "block")
            .property("margin-bottom", "0.5rem")
            .property("font-size", "0.9rem")
            .property("color", "rgba(255, 255, 255, 0.7)"),
        CssRule::new(".form-group input")
            .property("width", "100%")
            .property("background-color", "#2d2d2d")
            .property("border", "1px solid rgba(255, 255, 255, 0.1)")
            .property("color", "white")
            .property("padding", "0.75rem")
            .property("border-radius", "4px")
            .property("outline", "none"),
        CssRule::new(".form-group input:focus").property("border-color", "var(--bs-primary)"),
        CssRule::new(".modal-actions")
            .property("display", "flex")
            .property("justify-content", "flex-end")
            .property("gap", "1rem"),
        CssRule::new(".btn-primary")
            .property("background-color", "var(--bs-primary)")
            .property("color", "white")
            .property("border", "none")
            .property("padding", "0.5rem 1.5rem")
            .property("border-radius", "4px")
            .property("cursor", "pointer")
            .property("font-weight", "500"),
        CssRule::new(".btn-secondary")
            .property("background-color", "transparent")
            .property("color", "white")
            .property("border", "1px solid rgba(255, 255, 255, 0.2)")
            .property("padding", "0.5rem 1.5rem")
            .property("border-radius", "4px")
            .property("cursor", "pointer"),
        CssRule::new(".btn-secondary:hover")
            .property("background-color", "rgba(255, 255, 255, 0.05)"),
    ]
}
