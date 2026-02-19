use crate::styling::css::CssRule;

pub fn elements() -> Vec<CssRule> {
    vec![select()]
}

fn select() -> CssRule {
    CssRule::new("select")
        .property("background-color", "var(--neutral-900)")
        .property("border", "0.1rem solid var(--neutral-700)")
        .property("border-radius", "0.3rem")
        .property("font-size", "1rem")
        .property("padding", "0.5rem 1rem")
        .property("cursor", "pointer")
        .property("outline", "none")
        .property("transition", "border-color 0.5s ease")
        .child(
            CssRule::new("&:focus")
                .property("border-color", "var(--emerald-500)")
                .property("box-shadow", "0 0 0 0.1rem var(--emerald-800)"),
        )
}
