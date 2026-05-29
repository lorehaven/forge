use quench_srv::actix::routers::ui::common::css;
use quench_web::prelude::CssRule;

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
    let mut rules = Vec::new();
    rules.extend(css::layout_rules());
    rules.extend(utility_rules());
    rules.extend(tree_rules());
    rules.extend(table_rules());
    rules.extend(grid_rules());
    rules.extend(meta_rules());
    rules.extend(css::meta_rules());
    rules.extend(css::home_rules());
    rules.extend(css::login_rules());
    rules
}

fn utility_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".mr-2").property("margin-right", "0.5rem"),
        CssRule::new(".mt-4").property("margin-top", "1.5rem"),
        CssRule::new(".d-flex").property("display", "flex"),
        CssRule::new(".flex-column").property("flex-direction", "column"),
        CssRule::new(".h-100").property("height", "100%"),
        CssRule::new(".flex-1").property("flex", "1"),
        CssRule::new(".button-danger-sm")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("padding", "0.35rem 0.75rem")
            .property("background-color", "transparent")
            .property("color", "var(--bs-danger)")
            .property("border", "0.1rem solid var(--bs-danger)")
            .property("border-radius", "0.3rem")
            .property("font-size", "0.85rem")
            .property("font-weight", "500")
            .property("cursor", "pointer")
            .property("transition", "all 0.15s ease-in-out")
            .child(
                CssRule::new("&:hover")
                    .property("background-color", "var(--bs-danger)")
                    .property("color", "white"),
            )
            .child(
                CssRule::new("&:active")
                    .property("transform", "scale(0.96)")
                    .property("background-color", "var(--bs-danger-700)"),
            ),
    ]
}

fn tree_rules() -> Vec<CssRule> {
    vec![
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
            .property("display", "flex")
            .property("align-items", "center")
            .property("cursor", "pointer")
            .property("padding", "0.2rem 0")
            .child(
                CssRule::new("i")
                    .property("width", "1.5rem")
                    .property("flex-shrink", "0"),
            ),
        CssRule::new(".tag-list")
            .property("list-style", "none")
            .property("margin", "0.2rem 0")
            .property("padding-left", "1rem")
            .property("border-left", "0.1rem solid var(--bs-gray-700)"),
        CssRule::new(".repo-link")
            .property("display", "inline-flex")
            .property("padding", "0.15rem 0.3rem")
            .property("border-radius", "0.2rem")
            .property("text-decoration", "none")
            .property("color", "var(--bs-gray-300)")
            .child(CssRule::new("&:hover").property("background-color", "var(--bs-gray-700)")),
        CssRule::new(".repo-link.active")
            .property("background-color", "var(--bs-success-900)")
            .property("color", "var(--bs-gray-100)"),
    ]
}

fn table_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".table")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("min-height", "0")
            .property("height", "100%")
            .child(
                CssRule::new(".body")
                    .property("flex", "1 1 auto")
                    .property("min-height", "0"),
            ),
        CssRule::new(".table .cell")
            .property("min-width", "0")
            .property("overflow", "hidden")
            .property("white-space", "nowrap")
            .property("text-overflow", "ellipsis"),
        CssRule::new(".table .cell > *")
            .property("display", "block")
            .property("min-width", "0")
            .property("max-width", "100%")
            .property("overflow", "hidden")
            .property("white-space", "nowrap")
            .property("text-overflow", "ellipsis"),
        CssRule::new(".table .cell.actions,\n.table .cell.actions > *")
            .property("overflow", "visible")
            .property("white-space", "normal")
            .property("text-overflow", "clip"),
        CssRule::new(".table-scroll")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("flex", "1 1 auto")
            .property("min-height", "0")
            .property("overflow-x", "auto")
            .property("overflow-y", "auto"),
        CssRule::new(".table .header")
            .property("position", "sticky")
            .property("top", "0")
            .property("z-index", "1")
            .property("padding-right", "0.3rem"),
        CssRule::new(
            ".tags-grid .header,\n.tags-grid .body,\n.versions-grid .header,\n.versions-grid .body",
        )
        .property("min-width", "100%")
        .property("width", "max(100%, var(--table-min-width, 100%))"),
        CssRule::new(".files-toolbar")
            .property("display", "flex")
            .property("flex-wrap", "wrap")
            .property("gap", "0.5rem")
            .property("padding", "0.75rem")
            .property("border-bottom", "0.1rem solid var(--bs-gray-700)")
            .child(CssRule::new("input[type=\"file\"]").property("max-width", "20rem")),
        CssRule::new(".actions").property("gap", "0.6rem").child(
            CssRule::new("i")
                .property("cursor", "pointer")
                .property("color", "var(--bs-gray-300)")
                .child(CssRule::new("&:hover").property("color", "var(--bs-gray-100)")),
        ),
    ]
}

fn grid_rules() -> Vec<CssRule> {
    vec![
        // Docker tags grid
        CssRule::new(".tags-grid").property("--table-min-width", "66rem"),
        CssRule::new(".tags-grid")
            .child(CssRule::new(".header,\n.body > .row").property("display", "grid"))
            .child(
                CssRule::new(".header")
                    .property("grid-template-columns", "minmax(12rem, 2fr) minmax(14rem, 2fr) minmax(18rem, 3fr) minmax(7rem, 1fr)"),
            )
            .child(
                CssRule::new(".body > .row")
                    .property("grid-template-columns", "minmax(12rem, 2fr) minmax(14rem, 2fr) minmax(18rem, 3fr) minmax(7rem, 1fr)")
                    .child(CssRule::new("&.active").property("background-color", "var(--bs-gray-700)"))
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
        CssRule::new(".versions-grid").property("--table-min-width", "56rem"),
        CssRule::new(".versions-grid")
            .child(CssRule::new(".header,\n.body > .row").property("display", "grid"))
            .child(
                CssRule::new(".header")
                    .property("grid-template-columns", "minmax(12rem, 2fr) minmax(9rem, 1fr) minmax(16rem, 3fr) minmax(7rem, 1fr)"),
            )
            .child(
                CssRule::new(".body > .row")
                    .property("grid-template-columns", "minmax(12rem, 2fr) minmax(9rem, 1fr) minmax(16rem, 3fr) minmax(7rem, 1fr)")
                    .child(CssRule::new("&.active").property("background-color", "var(--bs-gray-700)"))
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
        // Files entries grid
        CssRule::new(".file-grid").property("--table-min-width", "68rem"),
        CssRule::new(".file-grid")
            .child(CssRule::new(".header,\n.body > .row").property("display", "grid"))
            .child(
                CssRule::new(".header")
                    .property("grid-template-columns", "minmax(0, 4rem) minmax(0, 24rem) minmax(0, 10rem) minmax(0, 10rem) minmax(0, 20rem)"),
            )
            .child(
                CssRule::new(".body > .row")
                    .property("grid-template-columns", "minmax(0, 4rem) minmax(0, 24rem) minmax(0, 10rem) minmax(0, 10rem) minmax(0, 20rem)")
                    .property("cursor", "pointer")
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
    ]
}

fn meta_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".tag-link")
            .property("display", "flex")
            .property("align-items", "center")
            .property("padding", "0.25rem 0.5rem")
            .property("border-radius", "0.25rem")
            .property("text-decoration", "none")
            .property("color", "var(--bs-gray-400)")
            .property("font-size", "0.9rem")
            .property("transition", "background-color 0.2s, color 0.2s")
            .child(CssRule::new("i").property("width", "1.2rem").property("flex-shrink", "0"))
            .child(
                CssRule::new("&:hover")
                    .property("color", "var(--bs-gray-100)")
                    .property("background-color", "var(--bs-gray-800)"),
            ),
        CssRule::new(".tag-link.active")
            .property("background-color", "var(--bs-success-900)")
            .property("color", "var(--bs-gray-100)")
            .child(CssRule::new("i").property("color", "var(--bs-gray-100)")),
        CssRule::new(".meta-row")
            .property("display", "grid")
            .property("grid-template-columns", "10rem minmax(0, 1fr)")
            .property("min-width", "100%")
            .property("width", "max-content")
            .property(r"gap", "0.75rem")
            .property("padding", "0.35rem 0"),
        CssRule::new(".meta-label").property("color", "var(--bs-gray-400)"),
        CssRule::new(".mono").property("font-family", "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, \"Liberation Mono\", \"Courier New\", monospace"),
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
    ]
}
