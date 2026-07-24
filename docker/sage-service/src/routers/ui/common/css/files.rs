use quench_web::prelude::CssRule;

pub fn files_rules() -> Vec<CssRule> {
    vec![
        // Paperclip attach button in the composer's extras row.
        CssRule::new(".chat-attach-area")
            .property("display", "flex")
            .property("align-items", "center"),
        CssRule::new(".chat-attach-btn")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("justify-content", "center")
            .property("width", "2rem")
            .property("height", "2rem")
            .property("border-radius", "0.5rem")
            .property("cursor", "pointer")
            .property("color", "var(--bs-gray-500)")
            .property("transition", "color 0.15s, background-color 0.15s"),
        CssRule::new(".chat-attach-btn:hover")
            .property("color", "var(--bs-gray-200)")
            .property("background-color", "rgba(255, 255, 255, 0.07)"),
        CssRule::new(".chat-attach-area.disabled")
            .property("opacity", "0.4")
            .property("pointer-events", "none"),
        // Staging area above the input holding chips for the next message.
        CssRule::new(".pending-attachments")
            .property("display", "flex")
            .property("flex-wrap", "wrap")
            .property("gap", "0.4rem")
            .property("padding", "0 0.25rem"),
        CssRule::new(".pending-attachments:empty").property("display", "none"),
        // Read-only attachment row inside a sent user message.
        CssRule::new(".message-attachments")
            .property("display", "flex")
            .property("flex-wrap", "wrap")
            .property("gap", "0.4rem")
            .property("margin-top", "0.5rem"),
        // The chip itself, shared by the staging area and message bubbles.
        CssRule::new(".attachment-chip")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("gap", "0.4rem")
            .property("max-width", "16rem")
            .property("padding", "0.3rem 0.5rem")
            .property("font-size", "0.75rem")
            .property("color", "var(--bs-gray-200)")
            .property("background-color", "rgba(255, 255, 255, 0.06)")
            .property("border", "1px solid rgba(255, 255, 255, 0.1)")
            .property("border-radius", "0.5rem"),
        CssRule::new(".attachment-icon")
            .property("color", "var(--bs-gray-400)")
            .property("font-size", "0.8rem"),
        // Thumbnail preview replacing the icon on image attachments.
        CssRule::new(".attachment-thumb")
            .property("width", "2.5rem")
            .property("height", "2.5rem")
            .property("object-fit", "cover")
            .property("border-radius", "0.35rem")
            .property("flex-shrink", "0"),
        CssRule::new(".attachment-name")
            .property("overflow", "hidden")
            .property("text-overflow", "ellipsis")
            .property("white-space", "nowrap"),
        CssRule::new(".attachment-size")
            .property("color", "var(--bs-gray-500)")
            .property("font-size", "0.68rem")
            .property("white-space", "nowrap"),
        // Upload/processing status badge on a chip.
        CssRule::new(".attachment-status")
            .property("font-size", "0.6rem")
            .property("text-transform", "uppercase")
            .property("letter-spacing", "0.04em")
            .property("padding", "0.05rem 0.35rem")
            .property("border-radius", "0.25rem")
            .property("border", "1px solid currentcolor")
            .property("white-space", "nowrap"),
        CssRule::new(".attachment-status-ready").property("color", "#4ade80"),
        CssRule::new(".attachment-status-processing").property("color", "#facc15"),
        CssRule::new(".attachment-status-uploaded").property("color", "#facc15"),
        CssRule::new(".attachment-status-failed").property("color", "#f87171"),
        CssRule::new(".attachment-retry")
            .property("border", "none")
            .property("background", "transparent")
            .property("color", "var(--bs-gray-500)")
            .property("cursor", "pointer")
            .property("padding", "0")
            .property("display", "inline-flex")
            .property("font-size", "0.72rem"),
        CssRule::new(".attachment-retry:hover").property("color", "#facc15"),
        CssRule::new(".attachment-remove")
            .property("border", "none")
            .property("background", "transparent")
            .property("color", "var(--bs-gray-500)")
            .property("cursor", "pointer")
            .property("padding", "0")
            .property("display", "inline-flex")
            .property("font-size", "0.75rem"),
        CssRule::new(".attachment-remove:hover").property("color", "#f87171"),
        CssRule::new(".attachment-download")
            .property("color", "var(--bs-gray-500)")
            .property("cursor", "pointer")
            .property("text-decoration", "none")
            .property("display", "inline-flex")
            .property("font-size", "0.72rem"),
        CssRule::new(".attachment-download:hover").property("color", "#4ade80"),
        // Source attribution shown under assistant messages.
        CssRule::new(".message-sources")
            .property("margin-top", "0.5rem")
            .property("padding-top", "0.5rem")
            .property("border-top", "1px solid rgba(255, 255, 255, 0.08)")
            .property("font-size", "0.72rem")
            .property("color", "var(--bs-gray-500)"),
        CssRule::new(".message-sources-label")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.35rem")
            .property("font-weight", "600")
            .property("text-transform", "uppercase")
            .property("letter-spacing", "0.04em")
            .property("margin-bottom", "0.25rem")
            .property("color", "var(--bs-gray-600)"),
        CssRule::new(".message-source-item")
            .property("display", "flex")
            .property("justify-content", "space-between")
            .property("gap", "0.5rem")
            .property("padding", "0.1rem 0"),
        CssRule::new(".message-source-score").property("color", "var(--bs-gray-600)"),
        // Sidebar "Files" section for an open project.
        CssRule::new(".files-section-header")
            .property("padding-top", "0.5rem")
            .property("padding-bottom", "0.35rem"),
        CssRule::new(".files-section-icon")
            .property("font-size", "0.7rem")
            .property("color", "rgba(255, 255, 255, 0.4)"),
        CssRule::new(".files-section-count")
            .property("font-size", "0.65rem")
            .property("color", "var(--bs-gray-600)"),
        // File rows are indented under the project like its conversations.
        CssRule::new(".project-file-item")
            .property("margin-left", "1.5rem")
            .property("font-size", "0.85rem"),
        CssRule::new(".file-item-link")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.4rem"),
        CssRule::new(".file-item-icon")
            .property("font-size", "0.75rem")
            .property("color", "var(--bs-gray-500)"),
        CssRule::new(".file-item-name")
            .property("overflow", "hidden")
            .property("text-overflow", "ellipsis")
            .property("white-space", "nowrap"),
        CssRule::new(".files-empty")
            .property("margin-left", "1.5rem")
            .property("padding", "0.4rem 0.75rem")
            .property("font-size", "0.8rem")
            .property("color", "var(--bs-gray-600)")
            .property("font-style", "italic"),
    ]
}
