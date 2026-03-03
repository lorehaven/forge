use super::ThemeSpec;
use crate::styling::css::CssRule;

pub struct BootstrapDarkTheme;

impl ThemeSpec for BootstrapDarkTheme {
    fn colors() -> Vec<CssRule> {
        vec![
            CssRule::new(":root")
                .property("--bs-warning", "#ffda6a")
                .property("--bs-success-500", "#75b798")
                .property("--bs-success-600", "#5f9f84")
                .property("--bs-success-700", "#4f8a72")
                .property("--bs-success-800", "#3d6f5b")
                .property("--bs-success-900", "#2b4f41")
                .property("--bs-danger", "#ea868f")
                .property("--bs-code-bg", "#495057")
                .property("--bs-gray-50", "#f8f9fa")
                .property("--bs-gray-100", "#f1f3f5")
                .property("--bs-gray-200", "#e9ecef")
                .property("--bs-gray-300", "#dee2e6")
                .property("--bs-gray-400", "#ced4da")
                .property("--bs-gray-500", "#adb5bd")
                .property("--bs-gray-600", "#6c757d")
                .property("--bs-gray-700", "#495057")
                .property("--bs-gray-800", "#343a40")
                .property("--bs-gray-900", "#212529")
                .property("--bs-gray-950", "#0f1114"),
            CssRule::new(".color-green").property("color", "var(--bs-success-700)"),
            CssRule::new(".color-yellow").property("color", "var(--bs-warning)"),
            CssRule::new(".color-red").property("color", "var(--bs-danger)"),
        ]
    }
}
