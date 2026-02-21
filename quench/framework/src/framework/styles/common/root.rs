use crate::styling::css::CssRule;

pub fn root() -> Vec<CssRule> {
    vec![
        CssRule::new("html,\nbody")
            .property("height", "100%")
            .property("margin", "0")
            .property("padding", "0")
            .property("user-select", "none"),
        CssRule::new(".app")
            .property("overflow", "hidden")
            .property("height", "100vh")
            .property("width", "100vw")
            .property("min-width", "100vw")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("background-color", "var(--bs-gray-800)"),
        CssRule::new("*")
            .property("font-family", "'Roboto', sans-serif")
            .property("color", "var(--bs-gray-300)"),
        CssRule::new("*")
            .child(
                CssRule::new("&::-webkit-scrollbar")
                    .property("width", "0.7rem")
                    .property("height", "0.7rem"),
            )
            .child(
                CssRule::new("&::-webkit-scrollbar-track")
                    .property("background", "var(--bs-gray-400)"),
            )
            .child(
                CssRule::new("&::-webkit-scrollbar-thumb")
                    .property("background-color", "var(--bs-gray-600)")
                    .property("border-radius", "0.3rem")
                    .property("border", "0.1rem solid var(--bs-gray-500)"),
            )
            .child(
                CssRule::new("&::-webkit-scrollbar-thumb:hover")
                    .property("background-color", "var(--bs-gray-500)"),
            ),
    ]
}
