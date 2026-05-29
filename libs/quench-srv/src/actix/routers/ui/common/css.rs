use quench_web::prelude::CssRule;

pub fn layout_rules() -> Vec<CssRule> {
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
            .property("padding", "0.5rem"),
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
        CssRule::new(".split-right")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "1rem")
            .property("min-height", "0"),
        CssRule::new(".right-top")
            .property("flex", "1 1 60%")
            .property("min-height", "0"),
        CssRule::new(".right-bottom")
            .property("flex", "0 0 35%")
            .property("min-height", "0"),
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
    ]
}

pub fn home_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".home-content").property("width", "100%"),
        CssRule::new(".home-container")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "1rem")
            .property("max-width", "84rem")
            .property("margin", "0")
            .property("padding", "0 3rem"),
        CssRule::new("@media screen and (max-width: 768px)")
            .child(CssRule::new(".home-container").property("padding", "1rem")),
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
    ]
}

pub fn login_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".login-layout")
            .property("min-height", "calc(100vh - 10rem)")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "center"),
        CssRule::new(".login-panel")
            .property("width", "100%")
            .property("max-width", "28rem"),
    ]
}

pub fn meta_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".meta-list")
            .property("padding", "0.75rem 1rem")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem")
            .property("flex", "1 1 auto")
            .property("min-height", "0")
            .property("overflow", "auto"),
        CssRule::new(".empty")
            .property("padding", "1rem")
            .property("color", "var(--bs-gray-500)"),
    ]
}
