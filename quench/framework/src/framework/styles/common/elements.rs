use crate::styling::css::CssRule;

pub fn elements() -> Vec<CssRule> {
    vec![
        h1(),
        h2(),
        h3(),
        form(),
        input(),
        password_wrapper(),
        button(),
        select(),
        section(),
        separator(),
        separator_or(),
        loader(),
        tabs(),
        slides_container(),
        binding_code_input(),
        table(),
        table_mobile(),
    ]
}

fn h1() -> CssRule {
    CssRule::new("h1").property("font-size", "3rem")
}

fn h2() -> CssRule {
    CssRule::new("h2").property("font-size", "2rem")
}

fn h3() -> CssRule {
    CssRule::new("h3")
        .property("font-size", "1.6rem")
        .property("font-weight", "400")
}

fn form() -> CssRule {
    CssRule::new("form")
        .property("display", "flex")
        .property("flex-direction", "column")
        .property("gap", "1rem")
        .property("width", "100%")
        .child(
            CssRule::new("@media screen and (max-width: 768px)")
                .child(CssRule::new("&").property("width", "90%")),
        )
        .child(
            CssRule::new(".error")
                .property("color", "var(--bs-danger)")
                .property("margin-top", "-0.8rem")
                .property("align-self", "flex-start"),
        )
}

fn input() -> CssRule {
    CssRule::new("input")
        .property("border-radius", "0.3rem")
        .property("border", "0.1rem var(--bs-gray-700) solid")
        .property("background-color", "var(--bs-gray-800)")
        .property("padding", "0.8rem")
        .property("font-size", "1.2rem")
        .property("transition", "border-color 0.3s ease")
        .child(
            CssRule::new("&:focus")
                .property("border-color", "var(--bs-success-700)")
                .property("outline", "none"),
        )
}

fn password_wrapper() -> CssRule {
    CssRule::new(".password-wrapper")
        .property("width", "100%")
        .property("display", "flex")
        .property("flex-direction", "row")
        .child(
            CssRule::new("input")
                .property("flex", "1 1 auto")
                .property("min-width", "0")
                .property("border-radius", "0.5rem 0 0 0.5rem"),
        )
        .child(
            CssRule::new("button")
                .property("max-width", "3rem")
                .property("border-radius", "0 0.5rem 0.5rem 0"),
        )
}

fn button() -> CssRule {
    CssRule::new("a.button,\nbutton")
        .property("display", "flex")
        .property("gap", "1rem")
        .property("align-items", "center")
        .property("justify-content", "center")
        .property("background-color", "var(--bs-success-900)")
        .property("color", "var(--bs-gray-300)")
        .property("border-radius", "0.25rem")
        .property("box-shadow", "none")
        .property("border", "none")
        .property("padding", "1rem")
        .property("text-decoration", "none")
        .property("font-size", "1.2rem")
        .property("cursor", "pointer")
        .property("transition", "background-color 0.3s ease")
        .child(CssRule::new("&:hover").property("background-color", "var(--bs-success-800)"))
        .child(CssRule::new("&:active").property("background-color", "var(--bs-success-700)"))
}

fn select() -> CssRule {
    CssRule::new("select")
        .property("background-color", "var(--bs-gray-900)")
        .property("border", "0.1rem solid var(--bs-gray-700)")
        .property("border-radius", "0.3rem")
        .property("font-size", "1rem")
        .property("padding", "0.5rem 1rem")
        .property("cursor", "pointer")
        .property("outline", "none")
        .property("transition", "border-color 0.5s ease")
        .child(
            CssRule::new("&:focus")
                .property("border-color", "var(--bs-success-500)")
                .property("box-shadow", "0 0 0 0.1rem var(--bs-success-800)"),
        )
}

fn section() -> CssRule {
    CssRule::new("section")
        .property("display", "flex")
        .property("flex-direction", "column")
        .property("justify-content", "center")
        .property("align-items", "center")
        .property("text-align", "center")
        .property("background-color", "var(--bs-gray-900)")
        .property("padding", "2rem 4rem")
        .property("border-radius", "0.5rem")
        .property("width", "100%")
        .child(
            CssRule::new("@media screen and (max-width: 768px)")
                .child(CssRule::new("&").property("padding", "2rem 0")),
        )
}

fn separator() -> CssRule {
    CssRule::new(".separator")
        .property("margin", "0.4rem 0")
        .property("border-bottom", "0.1rem var(--bs-gray-800) solid")
        .property("width", "100%")
}

fn separator_or() -> CssRule {
    CssRule::new(".separator-or")
        .property("display", "flex")
        .property("align-items", "center")
        .property("justify-content", "center")
        .property("gap", "1rem")
        .property("margin", "1rem 0")
}

fn loader() -> CssRule {
    CssRule::new(".loader").child(CssRule::new("i").property("font-size", "6rem"))
}

fn tabs() -> CssRule {
    CssRule::new(".tabs")
        .property("display", "flex")
        .property("gap", "1rem")
        .property("padding-bottom", "0.2rem")
        .property("border-bottom", "0.05rem solid var(--bs-gray-300)")
        .child(
            CssRule::new(".tab")
                .property("display", "flex")
                .property("justify-content", "center")
                .property("align-items", "center")
                .property("font-size", "1.2rem")
                .property("cursor", "pointer")
                .property("padding", "0 0.5rem 0.3rem 0")
                .property("border-bottom", "0.2rem solid var(--bs-gray-300)"),
        )
        .child(CssRule::new(".tab.active").property("border-bottom", "0.2rem solid var(--bs-success-700)"))
}

fn slides_container() -> CssRule {
    CssRule::new(".slides-container")
        .property("position", "relative")
        .property("width", "100%")
        .property("height", "100%")
        .child(
            CssRule::new(".slide")
                .property("position", "absolute")
                .property("inset", "0")
                .property("opacity", "0")
                .property("pointer-events", "none")
                .property("transform", "translateX(100%)")
                .property("transition", "opacity 0.25s ease, transform 0.25s ease")
                .child(
                    CssRule::new("&.active")
                        .property("opacity", "1")
                        .property("pointer-events", "auto")
                        .property("transform", "translateX(0)"),
                )
                .child(
                    CssRule::new("&.inactive")
                        .property("opacity", "0")
                        .property("transform", "translateX(-100%)"),
                ),
        )
}

fn binding_code_input() -> CssRule {
    CssRule::new(".binding-code-input")
        .property("display", "flex")
        .property("flex-direction", "row")
        .property("gap", "1rem")
        .property("align-items", "center")
        .property("justify-content", "center")
        .child(
            CssRule::new(".binding-code-digit")
                .property("height", "3rem")
                .property("width", "2rem")
                .property("border-radius", "0.25rem")
                .property("border", "0.1rem var(--bs-gray-700) solid")
                .property("background-color", "var(--bs-gray-800)")
                .property("color", "var(--bs-gray-300)")
                .property("font-size", "1.6rem")
                .property("text-align", "center")
                .property("transition", "border-color 0.3s ease")
                .property("appearance", "none")
                .property("-webkit-appearance", "none")
                .property("-moz-appearance", "textfield")
                .child(
                    CssRule::new("&:focus")
                        .property("border", "0.1rem solid var(--bs-success-700)")
                        .property("outline", "none"),
                )
                .child(
                    CssRule::new("@media screen and (max-width: 768px)").child(
                        CssRule::new("&")
                            .property("height", "2rem")
                            .property("width", "1rem"),
                    ),
                ),
        )
}

fn table() -> CssRule {
    CssRule::new(".table")
        .property("display", "flex")
        .property("flex-direction", "column")
        .property("overflow", "hidden")
        .property("height", "100%")
        .child(
            CssRule::new(".header")
                .property("display", "grid")
                .property("grid-template-columns", "repeat(auto-fit, minmax(10rem, 1fr))")
                .property("font-size", "1.4rem")
                .property("font-weight", "600")
                .property("background-color", "var(--bs-gray-700)")
                .property("border-radius", "0.25rem 0.25rem 0 0")
                .property("padding-right", "1rem"),
        )
        .child(
            CssRule::new(".body")
                .property("overflow-y", "scroll")
                .property("height", "100%")
                .property("border", "0.2rem solid var(--bs-gray-700)")
                .property("border-radius", "0 0 0.25rem 0.25rem")
                .child(
                    CssRule::new(".row")
                        .property("display", "grid")
                        .property("grid-template-columns", "repeat(auto-fit, minmax(10rem, 1fr))")
                        .child(
                            CssRule::new("&:not(:last-child)")
                                .property("border-bottom", "0.1rem solid var(--bs-gray-700)"),
                        ),
                )
                .child(
                    CssRule::new(".empty")
                        .property("padding", "1rem 0")
                        .property("text-align", "center")
                        .property("font-size", "1.6rem")
                        .property("font-weight", "600"),
                ),
        )
        .child(
            CssRule::new(".header,\n.body > .row")
                .child(
                    CssRule::new(".cell")
                        .property("display", "flex")
                        .property("align-items", "center")
                        .property("padding", "0.5rem")
                        .property("cursor", "default"),
                )
                .child(
                    CssRule::new(".cell.sortable")
                        .property("display", "flex")
                        .property("gap", "0.5rem")
                        .property("cursor", "pointer")
                        .child(
                            CssRule::new(".indicator")
                                .property("display", "inline-block")
                                .property("transform", "scaleY(-1)")
                                .property("opacity", "0")
                                .property(
                                    "transition",
                                    "transform 180ms ease, opacity 360ms ease",
                                ),
                        )
                        .child(CssRule::new(".indicator.active").property("opacity", "1"))
                        .child(CssRule::new(".indicator.desc").property("transform", "scaleY(1)")),
                )
                .child(
                    CssRule::new(".cell.actions")
                        .property("display", "flex")
                        .property("justify-content", "center")
                        .property("align-items", "center")
                        .property("gap", "0.3rem")
                        .child(
                            CssRule::new("i")
                                .property("cursor", "pointer")
                                .child(
                                    CssRule::new("&.disabled")
                                        .property("cursor", "not-allowed")
                                        .property("opacity", "0.5"),
                                )
                                .child(
                                    CssRule::new("&:hover")
                                        .property("color", "var(--bs-gray-200)"),
                                )
                                .child(
                                    CssRule::new("&:active")
                                        .property("color", "var(--bs-gray-100)"),
                                ),
                        ),
                )
                .child(
                    CssRule::new(".cell.buttons")
                        .property("display", "flex")
                        .child(CssRule::new("button").property("padding", "0.5rem 2rem"))
                        .child(
                            CssRule::new("button.disabled")
                                .property("background-color", "var(--bs-gray-600)")
                                .property("cursor", "not-allowed"),
                        ),
                ),
        )
}

fn table_mobile() -> CssRule {
    CssRule::new(".table-mobile")
        .property("display", "flex")
        .property("flex-direction", "column")
        .property("overflow", "hidden")
        .property("height", "100%")
        .property("gap", "0.5rem")
        .child(
            CssRule::new(".buttons-bar")
                .property("display", "flex")
                .property("flex-direction", "row")
                .property("justify-content", "space-evenly")
                .property("gap", "0.5rem")
                .child(
                    CssRule::new("button.action").property("width", "100%"),
                ),
        )
        .child(
            CssRule::new("button.disabled")
                .property("background-color", "var(--bs-gray-700)")
                .property("cursor", "not-allowed"),
        )
        .child(
            CssRule::new(".body")
                .property("display", "flex")
                .property("flex-direction", "column")
                .property("gap", "0.5rem")
                .property("overflow-y", "auto")
                .property("height", "100%")
                .property("font-size", "1.2rem")
                .child(
                    CssRule::new(".row")
                        .property("display", "flex")
                        .property("flex-direction", "column")
                        .property("border", "0.2rem solid var(--bs-gray-700)")
                        .property("border-radius", "0 0 0.25rem 0.25rem")
                        .child(
                            CssRule::new(".cell")
                                .property("display", "flex")
                                .property("justify-content", "space-between")
                                .property("padding", "0.5rem 1rem")
                                .property("gap", "1rem"),
                        )
                        .child(
                            CssRule::new(".cell.actions")
                                .property("display", "flex")
                                .property("justify-content", "center")
                                .property("gap", "1rem")
                                .child(
                                    CssRule::new("i")
                                        .property("font-size", "1.4rem")
                                        .property("cursor", "pointer")
                                        .child(
                                            CssRule::new("&.disabled")
                                                .property("cursor", "not-allowed")
                                                .property("opacity", "0.5"),
                                        ),
                                ),
                        )
                        .child(
                            CssRule::new(".empty")
                                .property("padding", "1rem 0")
                                .property("text-align", "center")
                                .property("font-size", "1.6rem")
                                .property("font-weight", "600"),
                        ),
                ),
        )
}
