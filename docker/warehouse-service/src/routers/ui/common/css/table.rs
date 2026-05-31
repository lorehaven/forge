use quench_web::prelude::CssRule;

pub fn table_rules() -> Vec<CssRule> {
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
