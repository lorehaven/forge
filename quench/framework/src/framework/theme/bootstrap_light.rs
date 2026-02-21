use super::ThemeSpec;
use crate::styling::css::CssRule;

pub struct BootstrapLightTheme;

impl ThemeSpec for BootstrapLightTheme {
    fn colors() -> Vec<CssRule> {
        vec![
            CssRule::new(":root")
                .property("--bs-warning", "#ffc107")
                .property("--bs-success-500", "#198754")
                .property("--bs-success-600", "#157347")
                .property("--bs-success-700", "#146c43")
                .property("--bs-success-800", "#0f5132")
                .property("--bs-success-900", "#0a3622")
                .property("--bs-danger", "#dc3545")
                .property("--bs-code-bg", "#343a40")
                .property("--bs-gray-50", "#0f1114")
                .property("--bs-gray-100", "#161a1d")
                .property("--bs-gray-200", "#1f252b")
                .property("--bs-gray-300", "#2b3035")
                .property("--bs-gray-400", "#343a40")
                .property("--bs-gray-500", "#495057")
                .property("--bs-gray-600", "#6c757d")
                .property("--bs-gray-700", "#adb5bd")
                .property("--bs-gray-800", "#ced4da")
                .property("--bs-gray-900", "#dee2e6")
                .property("--bs-gray-950", "#f8f9fa"),
            CssRule::new(".color-green").property("color", "var(--bs-success-700)"),
            CssRule::new(".color-yellow").property("color", "var(--bs-warning)"),
            CssRule::new(".color-red").property("color", "var(--bs-danger)"),
        ]
    }
}
