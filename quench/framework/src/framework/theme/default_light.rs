use super::ThemeSpec;
use crate::styling::css::CssRule;

pub struct DefaultLightTheme;

impl ThemeSpec for DefaultLightTheme {
    fn colors() -> Vec<CssRule> {
        vec![
            CssRule::new(":root")
                .property("--bs-warning", "#f59e0b")
                .property("--bs-success-500", "#10b981")
                .property("--bs-success-600", "#059669")
                .property("--bs-success-700", "#047857")
                .property("--bs-success-800", "#065f46")
                .property("--bs-success-900", "#064e3b")
                .property("--bs-danger", "#be123c")
                .property("--bs-code-bg", "#1f2937")
                .property("--bs-gray-50", "#0a0a0a")
                .property("--bs-gray-100", "#171717")
                .property("--bs-gray-200", "#262626")
                .property("--bs-gray-300", "#404040")
                .property("--bs-gray-400", "#525252")
                .property("--bs-gray-500", "#737373")
                .property("--bs-gray-600", "#a3a3a3")
                .property("--bs-gray-700", "#d4d4d4")
                .property("--bs-gray-800", "#e5e5e5")
                .property("--bs-gray-900", "#f5f5f5")
                .property("--bs-gray-950", "#fafafa"),
            CssRule::new(".color-green").property("color", "var(--bs-success-700)"),
            CssRule::new(".color-yellow").property("color", "var(--bs-warning)"),
            CssRule::new(".color-red").property("color", "var(--bs-danger)"),
        ]
    }
}
