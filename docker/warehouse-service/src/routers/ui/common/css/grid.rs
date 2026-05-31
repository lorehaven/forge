use quench_web::prelude::CssRule;

pub fn grid_rules() -> Vec<CssRule> {
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
