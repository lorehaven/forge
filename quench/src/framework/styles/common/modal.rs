use crate::styling::css::CssRule;

pub fn modal() -> Vec<CssRule> {
    vec![overlay(), modal_shared(), modal_side(), modal_center()]
}

fn overlay() -> CssRule {
    CssRule::new(".modal-overlay")
        .property("position", "fixed")
        .property("inset", "0")
        .property("top", "4rem")
        .property("bottom", "3rem")
        .property("z-index", "999")
        .property("background-color", "var(--neutral-800)")
        .property("opacity", "0")
        .property("transform", "translateX(-100%)")
        .property("transition", "transform 0.25s ease, opacity 0.25s ease")
        .child(
            CssRule::new("&.show")
                .property("transform", "translateX(0)")
                .property("opacity", "0.7"),
        )
}

fn modal_shared() -> CssRule {
    CssRule::new(".modal-side,\n.modal-center")
        .property("z-index", "1000")
        .property("background-color", "var(--neutral-900)")
        .property("border-radius", "0.25rem")
        .property("box-shadow", "0 0 0.2rem var(--neutral-500)")
        .property("opacity", "0")
        .property("overflow-y", "auto")
        .property("transition", "transform 0.25s ease, opacity 0.25s ease")
        .child(
            CssRule::new("&.show")
                .property("transform", "translateX(0)")
                .property("opacity", "1"),
        )
        .child(
            CssRule::new("@media screen and (max-width: 768px)")
                .child(CssRule::new("&").property("width", "100%")),
        )
        .child(
            CssRule::new(".modal-content")
                .property("padding", "2rem")
                .property("display", "flex")
                .property("flex-direction", "column")
                .property("gap", "0.8rem")
                .child(CssRule::new(".section-title").property("font-size", "1.6rem"))
                .child(CssRule::new(".section-subtitle").property("font-size", "1.3rem"))
                .child(
                    CssRule::new(".filter-field")
                        .property("display", "flex")
                        .property("flex-direction", "column"),
                ),
        )
}

fn modal_side() -> CssRule {
    CssRule::new(".modal-side")
        .property("position", "fixed")
        .property("left", "0")
        .property("top", "4rem")
        .property("bottom", "3rem")
        .property("width", "min(32rem, calc(100% - 2rem))")
        .property("transform", "translateX(-100%)")
        .child(
            CssRule::new(".modal-content")
                .child(
                    CssRule::new("label")
                        .property("font-size", "1.2rem")
                        .property("font-weight", "bold")
                        .property("margin-bottom", "-0.6rem"),
                )
                .child(CssRule::new("select").property("font-size", "1.2rem"))
                .child(
                    CssRule::new("button.nav")
                        .property("display", "flex")
                        .property("align-items", "center")
                        .property("gap", "0.5rem")
                        .property("text-decoration", "none")
                        .property("padding", "0.8rem 1rem")
                        .property("border-radius", "0.25rem")
                        .property("background-color", "var(--neutral-800)")
                        .property("border", "0.1rem var(--neutral-700) solid")
                        .property("transition", "background-color 0.3s ease")
                        .child(
                            CssRule::new("&:hover")
                                .property("background-color", "var(--neutral-700)"),
                        )
                        .child(
                            CssRule::new("&:active")
                                .property("background-color", "var(--neutral-600)"),
                        )
                        .child(
                            CssRule::new("&.current")
                                .property("border-color", "var(--emerald-900)")
                                .property("box-shadow", "0 0 0.6rem var(--emerald-900)"),
                        ),
                )
                .child(
                    CssRule::new("a.nav-entry-active")
                        .property("box-shadow", "0 0 0.6rem var(--emerald-900)"),
                ),
        )
}

fn modal_center() -> CssRule {
    CssRule::new(".modal-center")
        .property("position", "fixed")
        .property("inset", "auto")
        .property("z-index", "1000")
        .property("width", "min(36rem, 100%)")
        .property("transform", "translateX(-300%)")
        .child(
            CssRule::new("@media screen and (max-width: 768px)").child(
                CssRule::new("&")
                    .property("top", "0")
                    .property("bottom", "0"),
            ),
        )
        .child(
            CssRule::new("@media screen and (max-height: 1080px)").child(
                CssRule::new("&")
                    .property("top", "0")
                    .property("bottom", "0"),
            ),
        )
        .child(
            CssRule::new("form.modal-content")
                .property("width", "unset")
                .property("gap", "1rem")
                .child(
                    CssRule::new("img")
                        .property("width", "80%")
                        .property("border-radius", "0.5rem")
                        .property("align-self", "center"),
                )
                .child(
                    CssRule::new(".loader")
                        .property("display", "flex")
                        .property("flex-direction", "column")
                        .property("align-items", "center"),
                )
                .child(
                    CssRule::new(".code")
                        .property("display", "flex")
                        .property("justify-content", "center")
                        .property("align-items", "center")
                        .property("gap", "1rem")
                        .property("background-color", "var(--gray-800)")
                        .property("border-radius", "0.25rem")
                        .property("border", "var(--neutral-800)")
                        .property("padding", "1rem 2rem")
                        .property("font-size", "1.2rem")
                        .property("font-family", "monospace")
                        .property("width", "calc(100% - 4rem)")
                        .child(
                            CssRule::new("div")
                                .property("min-width", "0")
                                .property("white-space", "nowrap")
                                .property("flex", "1 1 0")
                                .property("overflow-x", "auto")
                                .child(
                                    CssRule::new("@media screen and (max-width: 768px)")
                                        .child(CssRule::new("&").property("overflow-x", "scroll")),
                                ),
                        )
                        .child(
                            CssRule::new("i")
                                .property("flex", "0 0 auto")
                                .property("cursor", "pointer")
                                .child(
                                    CssRule::new("&:hover").property("color", "var(--neutral-200)"),
                                )
                                .child(
                                    CssRule::new("&:active")
                                        .property("color", "var(--neutral-100)"),
                                ),
                        ),
                )
                .child(CssRule::new(".center-text").property("text-align", "center"))
                .child(
                    CssRule::new(".buttons")
                        .property("display", "flex")
                        .property("justify-content", "flex-end")
                        .property("gap", "1rem")
                        .child(
                            CssRule::new("button")
                                .property("padding", "1rem 2rem")
                                .child(
                                    CssRule::new("@media screen and (max-width: 768px)")
                                        .child(CssRule::new("&").property("width", "100%")),
                                ),
                        ),
                ),
        )
}
