//! The slide-out nav drawer's entry list.
//!
//! `.modal-overlay`/`.modal-side`/`.modal-content` themselves are styled by
//! quench-web's own shared stylesheet (`quench-web/src/framework/styles/
//! common/modal.rs`) - conveyor only needs to style what it put inside that
//! content: the two-entry list `common::nav::panel()` builds. Palantir's
//! equivalent (`side-nav-bar`/`side-nav-bar-entry`) lives in its own
//! hand-written SCSS, which conveyor has no access to and wouldn't want
//! anyway - conveyor's other pages are entirely `CssRule`, so this stays in
//! that language rather than introducing a second styling mechanism.

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
