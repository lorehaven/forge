use crate::styling::css::CssRule;

pub fn root() -> Vec<CssRule> {
    vec![
        CssRule::new(":root")
            .property("--q-shell-font", "'Roboto', sans-serif")
            .property("--q-shell-panel-radius", "0.35rem")
            .property("--q-shell-panel-border", "0.1rem solid var(--bs-gray-700)")
            .property("--q-shell-panel-shadow", "0 0 0 0.08rem var(--bs-gray-700)")
            .property("--q-shell-panel-bg", "var(--bs-gray-900)")
            .property("--q-shell-panel-bg-soft", "var(--bs-gray-800)")
            .property("--q-shell-panel-bg-strong", "var(--bs-gray-950)")
            .property("--q-shell-text", "var(--bs-gray-300)")
            .property("--q-shell-text-muted", "var(--bs-gray-500)")
            .property("--q-shell-accent", "var(--bs-success-700)"),
        CssRule::new("html,\nbody")
            .property("height", "100%")
            .property("margin", "0")
            .property("padding", "0")
            .property("user-select", "none"),
        CssRule::new(".app,\n.q-shell-app")
            .property("overflow", "hidden")
            .property("height", "100vh")
            .property("width", "100vw")
            .property("min-width", "100vw")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("background-color", "var(--q-shell-panel-bg-soft)"),
        CssRule::new("*")
            .property("font-family", "var(--q-shell-font)")
            .property("color", "var(--q-shell-text)"),
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
