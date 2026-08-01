//! The run list and the run page.
//!
//! Conveyor's only real addition to the estate's stylesheet beyond status
//! pills: a table of runs, and a log viewer that has to stay readable with ten
//! thousand lines in it.

use quench_web::prelude::CssRule;

pub fn runs_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".run-table")
            .property("width", "100%")
            .property("border-collapse", "collapse")
            .child(
                CssRule::new("th")
                    .property("text-align", "left")
                    .property("padding", "0.5rem 0.75rem")
                    .property("font-size", "0.8rem")
                    .property("text-transform", "uppercase")
                    .property("letter-spacing", "0.04em")
                    .property("color", "var(--bs-gray-500)")
                    .property("border-bottom", "0.1rem solid var(--bs-gray-700)"),
            )
            .child(
                CssRule::new("td")
                    .property("padding", "0.6rem 0.75rem")
                    .property("border-bottom", "0.1rem solid var(--bs-gray-800)")
                    .property("vertical-align", "middle"),
            )
            .child(CssRule::new("tr:hover td").property("background-color", "var(--bs-gray-800)")),
        // Shas, refs and durations line up only in a monospaced column.
        CssRule::new(".mono")
            .property(
                "font-family",
                "ui-monospace, SFMono-Regular, Menlo, monospace",
            )
            .property("font-size", "0.85rem"),
        CssRule::new(".muted").property("color", "var(--bs-gray-500)"),
        CssRule::new(".run-header")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.75rem")
            .property("flex-wrap", "wrap")
            .property("margin-bottom", "1rem"),
        CssRule::new(".run-meta")
            .property("display", "flex")
            .property("gap", "1.5rem")
            .property("flex-wrap", "wrap")
            .property("color", "var(--bs-gray-400)")
            .property("font-size", "0.9rem")
            .property("margin-bottom", "1rem"),
        // ---------------------------------------------------------------
        // Jobs
        // ---------------------------------------------------------------
        CssRule::new(".job")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.3rem")
            .property("margin-bottom", "0.75rem")
            .property("overflow", "hidden"),
        CssRule::new(".job-head")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.75rem")
            .property("padding", "0.6rem 0.9rem")
            .property("background-color", "var(--bs-gray-800)")
            .property("cursor", "pointer")
            .property("user-select", "none"),
        // A box only so the poll has something with an id to replace. `contents`
        // keeps its children as direct flex items of `.job-head`, so the row
        // lays out exactly as it did before there was anything to swap.
        CssRule::new(".job-state").property("display", "contents"),
        CssRule::new(".job-name")
            .property("font-weight", "600")
            .property("flex", "1"),
        CssRule::new(".job-body").property("padding", "0.5rem 0.9rem 0.9rem"),
        // `<details>` gives collapse and expand with no script; the marker is
        // replaced by the status pill, which already says more than a triangle.
        CssRule::new("details.job > summary")
            .property("list-style", "none")
            .child(CssRule::new("::-webkit-details-marker").property("display", "none")),
        CssRule::new(".job-reason")
            .property("color", "var(--bs-gray-400)")
            .property("font-size", "0.85rem")
            .property("padding", "0.5rem 0"),
        CssRule::new(".step")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.6rem")
            .property("padding", "0.25rem 0")
            .property("font-size", "0.85rem"),
        // ---------------------------------------------------------------
        // Logs
        // ---------------------------------------------------------------
        CssRule::new(".log")
            .property(
                "font-family",
                "ui-monospace, SFMono-Regular, Menlo, monospace",
            )
            .property("font-size", "0.82rem")
            .property("line-height", "1.45")
            .property("background-color", "var(--bs-gray-950)")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.3rem")
            .property("padding", "0.6rem 0.8rem")
            .property("max-height", "32rem")
            .property("overflow-y", "auto")
            // Build output is full of long lines; wrapping beats a horizontal
            // scrollbar nobody finds.
            .property("white-space", "pre-wrap")
            .property("word-break", "break-word"),
        CssRule::new(".log-line").property("display", "block"),
        CssRule::new(".log-stderr").property("color", "var(--bs-danger)"),
        CssRule::new(".log-note")
            .property("color", "var(--bs-warning)")
            .property("font-style", "italic"),
        CssRule::new(".log-empty").property("color", "var(--bs-gray-500)"),
        CssRule::new(".artifact")
            .property("display", "flex")
            .property("gap", "0.75rem")
            .property("align-items", "baseline")
            .property("padding", "0.3rem 0")
            .property("font-size", "0.9rem"),
        // ---------------------------------------------------------------
        // Repo scan
        // ---------------------------------------------------------------
        CssRule::new(".scan-grid")
            .property("display", "grid")
            .property(
                "grid-template-columns",
                "repeat(auto-fit, minmax(18rem, 1fr))",
            )
            .property("gap", "1rem"),
        CssRule::new(".scan-details")
            .property("margin", "0.5rem 0 0")
            .property("padding-left", "1.1rem")
            .property("font-size", "0.82rem")
            .property("color", "var(--bs-gray-400)")
            .child(CssRule::new("li").property("margin", "0.15rem 0")),
    ]
}
