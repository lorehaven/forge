use quench_web::prelude::CssRule;

pub fn tree_rules() -> Vec<CssRule> {
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
