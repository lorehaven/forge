//! Styles for the nav drawer's entry list only - the modal shell itself is
//! quench-web's shared stylesheet.

use quench_web::prelude::CssRule;

pub fn nav_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".side-nav-bar")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.4rem"),
        CssRule::new("a.side-nav-bar-entry")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.75rem")
            .property("padding", "0.7rem 1rem")
            .property("border-radius", "0.3rem")
            .property("text-decoration", "none")
            .property("color", "inherit")
            .property("background-color", "var(--bs-gray-800)")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("transition", "background-color 0.2s ease")
            .child(CssRule::new("&:hover").property("background-color", "var(--bs-gray-700)"))
            .child(
                CssRule::new("i")
                    .property("width", "1.2rem")
                    .property("text-align", "center"),
            ),
    ]
}
