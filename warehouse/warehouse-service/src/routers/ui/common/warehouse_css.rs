use quench::prelude::CssRule;

pub fn ensure_warehouse_css() {
    let css = warehouse_css_rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");

    let _ = std::fs::create_dir_all("dist/assets/css");
    let _ = std::fs::write("dist/assets/css/warehouse.css", css);
}

fn warehouse_css_rules() -> Vec<CssRule> {
    vec![
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
                    .property("grid-template-columns", "minmax(20rem, 28rem) minmax(0, 1fr)")
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
            .child(CssRule::new("a.button").property("padding", "0.6rem 1rem")),
        CssRule::new(".split-left,\n.split-right").property("min-height", "0"),
        CssRule::new(".split-right")
            .property("display", "grid")
            .property("grid-template-rows", "minmax(0, 1fr) minmax(0, 1fr)")
            .property("gap", "1rem")
            .child(
                CssRule::new("@media screen and (max-width: 1024px)")
                    .child(CssRule::new("&").property("grid-template-rows", "minmax(20rem, auto) minmax(14rem, auto)")),
            ),
        CssRule::new(".panel")
            .property("height", "100%")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.3rem")
            .property("background-color", "var(--bs-gray-900)")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("overflow", "hidden"),
        CssRule::new(".panel-title")
            .property("padding", "0.75rem 1rem")
            .property("font-weight", "600")
            .property("border-bottom", "0.1rem solid var(--bs-gray-700)")
            .property("background-color", "var(--bs-gray-800)"),
        CssRule::new(".tree-scroll")
            .property("flex", "1 1 auto")
            .property("min-height", "0")
            .property("height", "calc(100vh - 14rem)")
            .property("max-height", "calc(100vh - 14rem)")
            .property("overflow", "auto")
            .property("padding", "0.75rem"),
        CssRule::new(".repo-tree,\n.repo-tree ul")
            .property("list-style", "none")
            .property("margin", "0")
            .property("padding-left", "1rem"),
        CssRule::new(".repo-tree").property("padding-left", "0"),
        CssRule::new(".tree-folder")
            .property("cursor", "pointer")
            .property("padding", "0.2rem 0"),
        CssRule::new(".repo-link")
            .property("display", "inline-flex")
            .property("padding", "0.15rem 0.3rem")
            .property("border-radius", "0.2rem")
            .property("text-decoration", "none")
            .property("color", "var(--bs-gray-300)")
            .child(
                CssRule::new("&:hover")
                    .property("background-color", "var(--bs-gray-800)"),
            ),
        CssRule::new(".repo-link.active")
            .property("background-color", "var(--bs-success-900)")
            .property("color", "var(--bs-gray-100)"),
        CssRule::new(".table")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("min-height", "0")
            .property("height", "100%")
            .child(CssRule::new(".header").property("display", "grid"))
            .child(
                CssRule::new(".body")
                    .property("flex", "1 1 auto")
                    .property("overflow", "auto")
                    .property("min-height", "0"),
            ),
        // Docker tags grid
        CssRule::new(".tags-grid")
            .child(CssRule::new(".header,\n.body > .row").property("display", "grid"))
            .child(CssRule::new(".header").property("grid-template-columns", "2fr 2fr 3fr 1fr"))
            .child(
                CssRule::new(".body > .row")
                    .property("grid-template-columns", "2fr 2fr 3fr 1fr")
                    .child(CssRule::new("&.active").property("background-color", "var(--bs-gray-800)"))
                    .child(
                        CssRule::new("&:not(:last-child)")
                            .property("border-bottom", "0.1rem solid var(--bs-gray-700)"),
                    ),
            )
            .child(
                CssRule::new(".cell")
                    .property("padding", "0.45rem 0.55rem")
                    .property("display", "flex")
                    .property("align-items", "center"),
            ),
        // Crates versions grid  – version | status | checksum
        CssRule::new(".versions-grid")
            .child(CssRule::new(".header,\n.body > .row").property("display", "grid"))
            .child(CssRule::new(".header").property("grid-template-columns", "2fr 1fr 3fr 1fr"))
            .child(
                CssRule::new(".body > .row")
                    .property("grid-template-columns", "2fr 1fr 3fr 1fr")
                    .child(CssRule::new("&.active").property("background-color", "var(--bs-gray-800)"))
                    .child(
                        CssRule::new("&:not(:last-child)")
                            .property("border-bottom", "0.1rem solid var(--bs-gray-700)"),
                    ),
            )
            .child(
                CssRule::new(".cell")
                    .property("padding", "0.45rem 0.55rem")
                    .property("display", "flex")
                    .property("align-items", "center"),
            ),
        CssRule::new(".tag-link")
            .property("text-decoration", "none")
            .property("color", "var(--bs-gray-300)")
            .child(
                CssRule::new("&:hover")
                    .property("color", "var(--bs-gray-100)")
                    .property("text-decoration", "underline"),
            ),
        CssRule::new(".meta-list")
            .property("padding", "0.75rem 1rem")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem"),
        CssRule::new(".meta-row")
            .property("display", "grid")
            .property("grid-template-columns", "10rem 1fr")
            .property("gap", "0.75rem")
            .property("padding", "0.35rem 0")
            .child(
                CssRule::new("&:not(:last-child)")
                    .property("border-bottom", "0.1rem solid var(--bs-gray-800)"),
            ),
        CssRule::new(".meta-label").property("color", "var(--bs-gray-500)"),
        CssRule::new(".mono").property("font-family", "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, \"Liberation Mono\", \"Courier New\", monospace"),
        CssRule::new(".empty")
            .property("padding", "1rem")
            .property("color", "var(--bs-gray-500)"),
        // Dependency display within metadata panel
        CssRule::new(".meta-deps")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.5rem"),
        CssRule::new(".deps-group")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.15rem"),
        CssRule::new(".deps-group-label")
            .property("font-size", "0.75rem")
            .property("text-transform", "uppercase")
            .property("letter-spacing", "0.05em")
            .property("color", "var(--bs-gray-500)")
            .property("margin-bottom", "0.2rem"),
        CssRule::new(".dep-row")
            .property("font-size", "0.85rem")
            .property("color", "var(--bs-gray-300)")
            .property("padding", "0.1rem 0"),
        // Home / service index
        CssRule::new(".home-layout")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "2rem")
            .property("max-width", "56rem")
            .property("margin", "0 auto")
            .property("padding-top", "3rem"),
        CssRule::new(".home-header")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.4rem"),
        CssRule::new(".home-subtitle")
            .property("color", "var(--bs-gray-500)")
            .property("margin", "0"),
        CssRule::new(".home-grid")
            .property("display", "grid")
            .property("grid-template-columns", "repeat(auto-fill, minmax(18rem, 1fr))")
            .property("gap", "1rem"),
        CssRule::new(".home-card")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "space-between")
            .property("padding", "1.25rem 1.5rem")
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
            .property("gap", "0.35rem"),
        CssRule::new(".home-card-title")
            .property("font-size", "1.05rem")
            .property("font-weight", "600")
            .property("color", "var(--bs-gray-100)"),
        CssRule::new(".home-card-desc")
            .property("font-size", "0.85rem")
            .property("color", "var(--bs-gray-400)"),
        CssRule::new(".home-card-arrow")
            .property("font-size", "1.25rem")
            .property("color", "var(--bs-gray-500)")
            .property("flex-shrink", "0")
            .property("padding-left", "1rem"),
        // Login
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
