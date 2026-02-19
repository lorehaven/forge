use crate::styling::css::CssRule;

pub fn theme() -> String {
    colors()
        .into_iter()
        .map(|rule| rule.render())
        .collect::<Vec<_>>()
        .join("\n")
}

fn colors() -> Vec<CssRule> {
    vec![
        CssRule::new(":root")
            .property("--amber-500", "#f59e0b")
            .property("--emerald-500", "#10b981")
            .property("--emerald-600", "#059669")
            .property("--emerald-700", "#047857")
            .property("--emerald-800", "#065f46")
            .property("--emerald-900", "#064e3b")
            .property("--ruby-700", "#be123c")
            .property("--gray-800", "#1f2937")
            .property("--neutral-50", "#0a0a0a")
            .property("--neutral-100", "#171717")
            .property("--neutral-200", "#262626")
            .property("--neutral-300", "#404040")
            .property("--neutral-400", "#525252")
            .property("--neutral-500", "#737373")
            .property("--neutral-600", "#a3a3a3")
            .property("--neutral-700", "#d4d4d4")
            .property("--neutral-800", "#e5e5e5")
            .property("--neutral-900", "#f5f5f5")
            .property("--neutral-950", "#fafafa"),
        CssRule::new(".color-green").property("color", "var(--emerald-700)"),
        CssRule::new(".color-yellow").property("color", "var(--amber-500)"),
        CssRule::new(".color-red").property("color", "var(--ruby-700)"),
    ]
}
