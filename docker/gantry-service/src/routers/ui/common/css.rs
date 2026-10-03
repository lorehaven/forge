use quench_starter::actix::routers::ui::common::css as shared;
use quench_web::prelude::CssRule;

pub fn ensure_gantry_css() {
    forge_ui::write_css("gantry.css", &gantry_css_rules());
}

fn gantry_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(vec![
        CssRule::new("*,\n*::before,\n*::after").property("box-sizing", "border-box"),
    ]);
    rules.extend(shared::layout_rules());
    rules.extend(shared::home_rules());
    rules.extend(shared::login_rules());
    rules.extend(forge_ui::css_rules());
    rules.extend(shared::meta_rules());
    rules.extend(gantry_rules());
    rules
}

fn gantry_rules() -> Vec<CssRule> {
    let dot = |name: &str, color: &str| {
        CssRule::new(&format!(".gt-state-{name} .gt-dot")).property("background", color)
    };
    vec![
        // The shared home header is built for a big card grid; this is a working screen.
        CssRule::new(".home-container .home-header")
            .property("margin", "0 0 0.75rem")
            .property("padding", "0")
            .property("display", "flex")
            .property("flex-direction", "row")
            .property("align-items", "center")
            .property("justify-content", "flex-start")
            .property("text-align", "left")
            .property("gap", "0.75rem"),
        CssRule::new(".home-container .home-header h3")
            .property("flex", "0 0 auto")
            .property("text-align", "left")
            .property("margin", "0")
            .property("font-size", "1.2rem")
            .property("font-weight", "600"),
        CssRule::new(".gt-tabs")
            .property("display", "flex")
            .property("gap", "1.25rem")
            .property("margin-bottom", "1.25rem")
            .property("border-bottom", "1px solid var(--bs-gray-800, #333)"),
        CssRule::new(".gt-tab")
            .property("padding", "0.4rem 0.1rem")
            .property("color", "var(--bs-gray-400, #aaa)")
            .property("text-decoration", "none")
            .property("border-bottom", "2px solid transparent")
            .property("margin-bottom", "-1px"),
        CssRule::new(".gt-tab-active")
            .property("color", "inherit")
            .property("border-bottom-color", "currentColor"),
        CssRule::new(".gt-table")
            .property("width", "100%")
            .property("border-collapse", "collapse")
            .property("font-size", "0.92rem"),
        CssRule::new(".gt-table th")
            .property("text-align", "left")
            .property("padding", "0.35rem 0.75rem")
            .property("font-size", "0.75rem")
            .property("font-weight", "500")
            .property("text-transform", "uppercase")
            .property("letter-spacing", "0.04em")
            .property("color", "var(--bs-gray-500, #888)")
            .property("border-bottom", "1px solid var(--bs-gray-800, #333)"),
        CssRule::new(".gt-table td")
            .property("text-align", "left")
            .property("padding", "0.6rem 0.75rem")
            .property("vertical-align", "middle")
            .property("border-bottom", "1px solid var(--bs-gray-900, #222)"),
        CssRule::new(".gt-table tr:hover td").property("background", "rgba(255, 255, 255, 0.03)"),
        CssRule::new(".gt-table a").property("color", "inherit"),
        CssRule::new(".gt-desc")
            .property("margin", "0.1rem 0 0")
            .property("font-size", "0.8rem")
            .property("color", "var(--bs-gray-500, #888)"),
        CssRule::new(".gt-muted").property("color", "var(--bs-gray-500, #888)"),
        CssRule::new(".gt-right")
            .property("display", "flex")
            .property("justify-content", "flex-end")
            .property("gap", "0.4rem")
            .property("white-space", "nowrap"),
        CssRule::new(".gt-right form, .gt-right .gt-inline")
            .property("width", "auto")
            .property("margin", "0")
            .property("display", "inline-block"),
        // A state is a coloured dot and a word, not a pill.
        CssRule::new(".gt-state")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("gap", "0.45rem")
            .property("white-space", "nowrap"),
        CssRule::new(".gt-dot")
            .property("width", "0.55rem")
            .property("height", "0.55rem")
            .property("border-radius", "50%")
            .property("background", "#8b949e"),
        dot("running", "#3fb950"),
        dot("succeeded", "#3fb950"),
        dot("current", "#3fb950"),
        dot("update_available", "#d29922"),
        dot("partial", "#d29922"),
        dot("mixed", "#db6d28"),
        dot("failed", "#f85149"),
        dot("rolled_back", "#f85149"),
        dot("ahead", "#a371f7"),
        dot("unlisted", "#a371f7"),
        dot("stopped", "#8b949e"),
        dot("queued", "#8b949e"),
        dot("cancelled", "#8b949e"),
        dot("not_installed", "#484f58"),
        dot("absent", "#484f58"),
        CssRule::new(".gt-state-running .gt-dot, .gt-state-queued .gt-dot")
            .property("box-shadow", "0 0 0 3px rgba(88, 166, 255, 0.0)"),
        CssRule::new(".gt-yanked")
            .property("color", "#f85149")
            .property("font-size", "0.8rem"),
        // Buttons: quiet by default, one coloured one per page.
        CssRule::new(".gt-btn")
            .property("appearance", "none")
            .property("background", "transparent")
            .property("color", "inherit")
            .property("border", "1px solid var(--bs-gray-600, #555)")
            .property("border-radius", "6px")
            .property("padding", "0.25rem 0.85rem")
            .property("font-size", "0.85rem")
            .property("line-height", "1.4")
            .property("white-space", "nowrap")
            .property("text-decoration", "none")
            .property("display", "inline-block")
            .property("cursor", "pointer"),
        CssRule::new(".gt-btn:hover").property("background", "rgba(255, 255, 255, 0.07)"),
        CssRule::new(".gt-btn-primary")
            .property("background", "#238636")
            .property("border-color", "#238636")
            .property("color", "#fff"),
        CssRule::new(".gt-btn-primary:hover").property("background", "#2ea043"),
        CssRule::new(".gt-btn-danger")
            .property("color", "#f85149")
            .property("border-color", "rgba(248, 81, 73, 0.5)"),
        CssRule::new(".gt-btn-danger:hover").property("background", "rgba(248, 81, 73, 0.12)"),
        CssRule::new(".gt-inline")
            .property("display", "inline-block")
            .property("margin", "0"),
        CssRule::new(".gt-inline + .gt-inline").property("margin-left", "0.4rem"),
        CssRule::new(".gt-notice")
            .property("padding", "0.5rem 0.8rem")
            .property("border-radius", "6px")
            .property("margin", "0 0 1rem")
            .property("font-size", "0.9rem"),
        CssRule::new(".gt-notice-error").property("background", "rgba(248, 81, 73, 0.15)"),
        CssRule::new(".gt-notice-ok").property("background", "rgba(63, 185, 80, 0.15)"),
        CssRule::new(".gt-card")
            .property("border", "1px solid var(--bs-gray-800, #333)")
            .property("border-radius", "8px")
            .property("padding", "0.9rem 1.1rem")
            .property("margin", "0 0 1rem"),
        CssRule::new(".gt-card ul")
            .property("margin", "0")
            .property("padding-left", "1.1rem")
            .property("line-height", "1.7"),
        CssRule::new(".gt-bar")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.75rem")
            .property("margin", "1rem 0"),
        CssRule::new(".gt-bar-end").property("justify-content", "flex-end"),
        CssRule::new(".gt-meta")
            .property("margin", "0 0 1rem")
            .property("font-size", "0.85rem")
            .property("color", "var(--bs-gray-500, #888)"),
        CssRule::new(".gt-crumb")
            .property("font-size", "0.85rem")
            .property("margin", "0 0 0.5rem")
            .property("color", "var(--bs-gray-500, #888)"),
        CssRule::new(".gt-crumb a").property("color", "inherit"),
        CssRule::new("details.gt-details").property("margin", "0 0 1rem"),
        CssRule::new("details.gt-details > summary")
            .property("cursor", "pointer")
            .property("color", "var(--bs-gray-400, #aaa)")
            .property("font-size", "0.9rem")
            .property("padding", "0.25rem 0"),
        CssRule::new(".gt-list")
            .property("margin", "0.5rem 0 0")
            .property("padding-left", "1.4rem")
            .property("line-height", "1.7")
            .property("font-size", "0.9rem"),
        CssRule::new(".gt-form")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.6rem")
            .property("margin", "0.5rem 0"),
        CssRule::new(".gt-inline-form")
            .property("flex-direction", "row")
            .property("align-items", "center")
            .property("flex-wrap", "wrap")
            .property("margin", "0 0 1rem"),
        CssRule::new(".gt-field-row")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.25rem"),
        CssRule::new(".gt-field-row label, .gt-inline-form label")
            .property("font-size", "0.82rem")
            .property("color", "var(--bs-gray-400, #aaa)"),
        CssRule::new(".gt-form textarea")
            .property("width", "100%")
            .property("font-family", "monospace")
            .property("font-size", "0.85rem"),
        CssRule::new(".gt-log")
            .property("background", "rgba(0, 0, 0, 0.35)")
            .property("padding", "0.8rem 1rem")
            .property("border-radius", "8px")
            .property("overflow", "auto")
            .property("max-height", "30rem")
            .property("font-size", "0.8rem")
            .property("line-height", "1.5")
            .property("white-space", "pre-wrap")
            .property("margin", "0"),
        CssRule::new(".gt-group").property("margin", "0 0 2rem"),
        CssRule::new(".gt-group-head")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "space-between")
            .property("gap", "1rem")
            .property("margin", "0 0 0.5rem")
            .property("flex-wrap", "wrap"),
        CssRule::new(".gt-group-title")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.9rem"),
        CssRule::new(".gt-group-title a")
            .property("color", "inherit")
            .property("font-size", "1.05rem"),
        CssRule::new(".gt-deps")
            .property("border", "1px solid var(--bs-gray-800, #333)")
            .property("border-radius", "8px")
            .property("margin", "0 0 0.75rem")
            .property("padding", "0.2rem 0.9rem"),
        CssRule::new(".gt-dep")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.9rem")
            .property("padding", "0.45rem 0"),
        CssRule::new(".gt-dep .gt-right").property("margin-left", "auto"),
        CssRule::new(".gt-notice-info").property("background", "rgba(88, 166, 255, 0.12)"),
        CssRule::new(".gt-notice-info a").property("color", "inherit"),
        CssRule::new(".gt-group-actions")
            .property("display", "flex")
            .property("gap", "0.5rem"),
        // Every text field, on the same dark ground as the log, not the browser's white.
        CssRule::new(".gt-form textarea, .gt-form input[type=text], .gt-form select, .gt-inline-form select, .gt-yaml")
            .property("background", "rgba(0, 0, 0, 0.35)")
            .property("color", "inherit")
            .property("border", "1px solid var(--bs-gray-700, #444)")
            .property("border-radius", "6px")
            .property("padding", "0.5rem 0.65rem"),
        CssRule::new(".gt-form textarea:focus, .gt-form input:focus, .gt-form select:focus")
            .property("outline", "none")
            .property("border-color", "#58a6ff"),
        CssRule::new(".gt-yaml")
            .property("width", "100%")
            .property("font-family", "monospace")
            .property("font-size", "0.82rem")
            .property("line-height", "1.5")
            .property("tab-size", "2"),
        dot("synced", "#3fb950"),
        dot("edited", "#d29922"),
        dot("missing", "#f85149"),
        dot("extra", "#a371f7"),
        dot("hidden", "#8b949e"),
        CssRule::new(".gt-section")
            .property("margin", "1.75rem 0 0.5rem")
            .property("font-size", "0.8rem")
            .property("font-weight", "500")
            .property("text-transform", "uppercase")
            .property("letter-spacing", "0.04em")
            .property("color", "var(--bs-gray-500, #888)"),
    ]
}
