//! Status pills for the six run/job/step states, shared across every listing page.
//! Colours use theme variables, not literals, so pills follow theme changes.

use quench_web::prelude::CssRule;

pub fn status_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".status")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("gap", "0.4rem")
            .property("padding", "0.15rem 0.6rem")
            .property("border-radius", "999px")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("background-color", "var(--bs-gray-800)")
            .property("font-size", "0.8rem")
            .property("font-weight", "600")
            .property("line-height", "1.6")
            .property("text-transform", "uppercase")
            .property("letter-spacing", "0.03em")
            .property("white-space", "nowrap")
            .child(
                CssRule::new("&::before")
                    .property("content", "''")
                    .property("width", "0.5rem")
                    .property("height", "0.5rem")
                    .property("border-radius", "50%")
                    .property("background-color", "currentColor"),
            ),
        // Queued has no colour of its own on purpose: nothing has happened yet,
        // and a row that has not started should not draw the eye.
        CssRule::new(".status-queued").property("color", "var(--bs-gray-500)"),
        CssRule::new(".status-running")
            .property("color", "var(--bs-warning)")
            .child(CssRule::new("&::before").property("animation", "conveyor-pulse 1.2s infinite")),
        CssRule::new(".status-success").property("color", "var(--bs-success-500)"),
        CssRule::new(".status-failed").property("color", "var(--bs-danger)"),
        // Neutral: neither says anything about the code. Cancelled is brighter.
        CssRule::new(".status-cancelled").property("color", "var(--bs-gray-300)"),
        CssRule::new(".status-skipped").property("color", "var(--bs-gray-600)"),
        CssRule::new("@keyframes conveyor-pulse")
            .child(CssRule::new("0%, 100%").property("opacity", "1"))
            .child(CssRule::new("50%").property("opacity", "0.25")),
        // Respect a system-level request for less motion: the pulse is
        // decoration, and the colour already carries the meaning.
        CssRule::new("@media (prefers-reduced-motion: reduce)").child(
            CssRule::new(".status-running::before").property("animation", "none !important"),
        ),
    ]
}
