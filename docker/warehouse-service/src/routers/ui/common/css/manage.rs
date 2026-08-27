//! Styles for the file-storage and APK management pages: the quota bar, the
//! provisioning/edit forms, the in-browser file list, and the storage-delete
//! confirm modal (the image-modal rules in `utility.rs` are keyed to their own
//! id, so this restates the shape for `#confirm-delete-storage-modal`).

use quench_web::prelude::CssRule;

pub fn manage_rules() -> Vec<CssRule> {
    let mut rules = vec![
        // Scroll container for a detail panel that stacks metadata, forms and a
        // file list rather than a single `.meta-list`.
        CssRule::new(".manage-scroll")
            .property("padding", "0.75rem 1rem")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "1rem")
            .property("flex", "1 1 auto")
            .property("min-height", "0")
            .property("overflow", "auto"),
        CssRule::new(".panel-subtitle")
            .property("font-weight", "600")
            .property("font-size", "0.9rem")
            .property("text-transform", "uppercase")
            .property("letter-spacing", "0.04em")
            .property("color", "var(--bs-gray-400)")
            .property("margin-bottom", "0.5rem"),
        // Left-list annotations.
        CssRule::new(".storage-owner")
            .property("color", "var(--bs-gray-500)")
            .property("font-size", "0.85rem"),
        CssRule::new(".storage-badge")
            .property("margin-left", "0.5rem")
            .property("padding", "0.05rem 0.4rem")
            .property("border", "0.1rem solid var(--bs-gray-600)")
            .property("border-radius", "0.25rem")
            .property("font-size", "0.7rem")
            .property("text-transform", "uppercase")
            .property("color", "var(--bs-gray-400)"),
        // Quota bar.
        CssRule::new(".quota-bar")
            .property("position", "relative")
            .property("width", "100%")
            .property("height", "1.25rem")
            .property("background-color", "var(--bs-gray-800)")
            .property("border-radius", "0.25rem")
            .property("overflow", "hidden")
            .child(
                CssRule::new(".quota-bar-fill")
                    .property("position", "absolute")
                    .property("inset", "0 auto 0 0")
                    .property("background-color", "var(--bs-success-700)"),
            )
            .child(
                CssRule::new(".quota-bar-label")
                    .property("position", "relative")
                    .property("display", "block")
                    .property("padding", "0 0.4rem")
                    .property("font-size", "0.75rem")
                    .property("line-height", "1.25rem")
                    .property("white-space", "nowrap"),
            ),
        // Provision / edit forms.
        CssRule::new(".storage-form")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.6rem")
            .property("padding-top", "1rem")
            .property("border-top", "0.1rem solid var(--bs-gray-700)"),
        CssRule::new(".field-row")
            .property("display", "grid")
            .property("grid-template-columns", "12rem minmax(0, 1fr)")
            .property("align-items", "center")
            .property("gap", "0.75rem")
            .child(
                CssRule::new("input[type=\"text\"],\ninput[type=\"number\"]")
                    .property("width", "100%")
                    .property("padding", "0.35rem 0.5rem")
                    .property("background-color", "var(--bs-gray-900)")
                    .property("border", "0.1rem solid var(--bs-gray-700)")
                    .property("border-radius", "0.25rem")
                    .property("color", "inherit"),
            ),
        CssRule::new(".field-label")
            .property("color", "var(--bs-gray-400)")
            .property("font-size", "0.9rem"),
        // File list.
        CssRule::new(".file-list")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("list-style", "none")
            .property("margin", "0")
            .property("padding", "0"),
        CssRule::new(".file-row")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.75rem")
            .property("padding", "0.3rem 0")
            .property("border-bottom", "0.1rem solid var(--bs-gray-800)")
            .child(
                CssRule::new(".file-name")
                    .property("flex", "1 1 auto")
                    .property("min-width", "0"),
            )
            .child(
                CssRule::new(".file-size")
                    .property("color", "var(--bs-gray-500)")
                    .property("font-size", "0.8rem")
                    .property("white-space", "nowrap"),
            )
            .child(
                CssRule::new(".file-download")
                    .property("color", "var(--bs-info)")
                    .property("font-size", "0.8rem")
                    .property("text-decoration", "none"),
            ),
        CssRule::new(".file-truncated")
            .property("padding-top", "0.5rem")
            .property("color", "var(--bs-gray-500)")
            .property("font-size", "0.8rem")
            .property("font-style", "italic"),
    ];

    rules.push(confirm_modal_rule("#confirm-delete-storage-modal"));
    rules
}

/// The image-modal styling from `utility.rs`, restated for another id. Kept in
/// sync by shape rather than shared because the source rules hardcode their
/// own selector.
fn confirm_modal_rule(id: &str) -> CssRule {
    CssRule::new(id)
        .property("position", "fixed")
        .property("inset", "0")
        .property("display", "none")
        .property("z-index", "9999")
        .child(CssRule::new("&.open").property("display", "flex"))
        .child(
            CssRule::new(".confirm-modal-backdrop")
                .property("position", "absolute")
                .property("inset", "0")
                .property("background", "rgba(0,0,0,0.7)")
                .property("border", "none"),
        )
        .child(
            CssRule::new(".confirm-modal-content")
                .property("position", "relative")
                .property("margin", "auto")
                .property("width", "min(30rem, calc(100% - 2rem))")
                .property("background", "var(--bs-gray-900)")
                .property("border", "0.0625rem solid var(--bs-gray-700)")
                .property("border-radius", "0.5rem")
                .property("overflow", "hidden"),
        )
        .child(
            CssRule::new(".confirm-modal-header")
                .property("display", "flex")
                .property("justify-content", "space-between")
                .property("align-items", "center")
                .property("padding", "1rem 1.25rem")
                .property("border-bottom", "0.0625rem solid var(--bs-gray-700)"),
        )
        .child(
            CssRule::new(".confirm-modal-title")
                .property("font-size", "1.25rem")
                .property("font-weight", "700")
                .property("font-family", "monospace"),
        )
        .child(
            CssRule::new(".confirm-modal-close")
                .property("border", "none")
                .property("background", "transparent")
                .property("color", "var(--bs-gray-100)")
                .property("font-size", "1.3rem")
                .property("cursor", "pointer")
                .property("padding", "0.35rem")
                .property("line-height", "1"),
        )
        .child(
            CssRule::new(".confirm-modal-body")
                .property("display", "flex")
                .property("flex-direction", "column")
                .property("align-items", "center")
                .property("padding", "1rem")
                .child(
                    CssRule::new("p")
                        .property("text-align", "center")
                        .property("margin", "0 0 1rem 0"),
                )
                .child(
                    CssRule::new(".confirm-delete-target")
                        .property("font-family", "monospace")
                        .property("font-weight", "700")
                        .property("color", "var(--bs-warning)")
                        .property("margin-bottom", "1.5rem")
                        .property("word-break", "break-all")
                        .property("text-align", "center"),
                )
                .child(
                    CssRule::new(".confirm-actions")
                        .property("display", "flex")
                        .property("flex-direction", "row")
                        .property("gap", "1rem")
                        .property("justify-content", "center")
                        .property("align-items", "center")
                        .property("width", "100%")
                        .child(
                            CssRule::new(".button")
                                .property("min-width", "5.5rem")
                                .property("padding", "0.4rem 0.75rem")
                                .property("font-size", "0.85rem"),
                        ),
                ),
        )
}
