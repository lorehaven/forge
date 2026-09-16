use quench_starter::actix::routers::ui::common::css as shared;
use quench_web::prelude::CssRule;

pub fn ensure_workbench_css() {
    let css = workbench_css_rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");

    let _ = std::fs::create_dir_all("dist/assets/css");
    let _ = std::fs::write("dist/assets/css/workbench.css", css);
}

fn workbench_css_rules() -> Vec<CssRule> {
    let mut rules = Vec::new();
    rules.extend(reset_rules());
    rules.extend(shared::layout_rules());
    rules.extend(shared::home_rules());
    rules.extend(shared::login_rules());
    rules.extend(shared::meta_rules());
    rules.extend(form_rules());
    rules.extend(board_rules());
    rules.extend(modal_rules());
    rules.extend(link_rules());
    rules
}

/// Forces `border-box`; the shared theme's default `content-box` makes a
/// `.wb-form`'s padding overflow the panel's border.
fn reset_rules() -> Vec<CssRule> {
    vec![CssRule::new("*,\n*::before,\n*::after").property("box-sizing", "border-box")]
}

fn form_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".wb-form-panel").property("margin-top", "1rem"),
        CssRule::new(".wb-form")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.6rem")
            .property("padding", "1rem"),
        // Block, not inline - labels sit above their control, freeing full
        // width so the assignee picker's select+button don't overflow.
        CssRule::new(".wb-form label")
            .property("display", "block")
            .property("font-size", "0.85rem")
            .property("color", "var(--bs-gray-400)")
            .property("margin-bottom", "0.3rem"),
        CssRule::new(".wb-field-row")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.3rem"),
        // Assignee select + "assign to me" shortcut share this control column.
        CssRule::new(".wb-field-control")
            .property("display", "flex")
            .property("flex", "1 1 auto")
            .property("min-width", "0")
            .property("flex-wrap", "wrap")
            .property("align-items", "center")
            .property("gap", "0.5rem"),
        // Lets the select shrink below content width instead of overflowing.
        CssRule::new(".wb-field-control select")
            .property("flex", "1 1 auto")
            .property("min-width", "0"),
        CssRule::new(".wb-assign-me")
            .property("flex", "0 0 auto")
            .property("padding", "0.6rem 0.8rem")
            .property("font-size", "0.8rem")
            .property("border-radius", "0.3rem")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("background-color", "var(--bs-gray-800)")
            .property("color", "var(--bs-gray-300)")
            .property("cursor", "pointer")
            .property("white-space", "nowrap")
            .child(CssRule::new("&:hover").property("background-color", "var(--bs-gray-700)")),
        // Shared theme styles input/select dark but not textarea; matched here.
        CssRule::new(".wb-form textarea")
            .property("border-radius", "0.3rem")
            .property("border", "0.1rem var(--bs-gray-700) solid")
            .property("background-color", "var(--bs-gray-800)")
            .property("color", "var(--bs-gray-100)")
            .property("padding", "0.8rem")
            .property("font-size", "1.2rem")
            .property("font-family", "inherit")
            .property("resize", "vertical")
            .property("transition", "border-color 0.3s ease")
            .child(
                CssRule::new("&:focus")
                    .property("border-color", "var(--bs-success-700)")
                    .property("outline", "none"),
            ),
        CssRule::new(".wb-form-row")
            .property("display", "flex")
            .property("gap", "1rem")
            .property("flex-wrap", "wrap"),
        CssRule::new(".wb-form-row > *").property("flex", "1 1 12rem"),
        // Opts out of the shared full-width button rule via `align-self`.
        CssRule::new(".wb-submit")
            .property("align-self", "flex-end")
            .property("padding", "0.6rem 1.2rem")
            .property("font-size", "0.95rem"),
        CssRule::new(".wb-notice")
            .property("padding", "0.6rem 1rem")
            .property("border-radius", "0.3rem")
            .property("margin-bottom", "0.5rem"),
        CssRule::new(".wb-notice-error")
            .property("background-color", "#7a1f28")
            .property("color", "#fff"),
        CssRule::new(".wb-notice-ok")
            .property("background-color", "#245c33")
            .property("color", "#fff"),
    ]
}

fn board_rules() -> Vec<CssRule> {
    vec![
        // Columns own their own scrolling, so the ancestor chain must respect
        // `.content`'s real height instead of growing past it (two scrollbars).
        CssRule::new(".content:has(.wb-board)").property("overflow", "hidden"),
        CssRule::new(".content-inner:has(.wb-board)")
            .property("height", "100%")
            .property("min-height", "0")
            .property("justify-content", "flex-start")
            .property("align-items", "stretch"),
        CssRule::new(".page:has(.wb-board)")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("height", "100%")
            .property("min-height", "0"),
        CssRule::new("content.home-content:has(.wb-board)")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("height", "100%")
            .property("min-height", "0"),
        CssRule::new(".home-container:has(.wb-board)")
            .property("flex", "1 1 auto")
            .property("min-height", "0"),
        // The one scrollbar. Grid, not flex: a scrolling flex container
        // suppresses stretched items' content-based min-size.
        CssRule::new(".wb-board")
            .property("display", "grid")
            .property("grid-auto-flow", "column")
            .property("grid-auto-columns", "minmax(16rem, 1fr)")
            .property("align-items", "stretch")
            .property("gap", "1rem")
            .property("width", "100%")
            .property("flex", "1 1 auto")
            .property("min-height", "0")
            .property("overflow", "auto"),
        CssRule::new(".wb-column")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.4rem")
            .property("background-color", "var(--bs-gray-900)")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.6rem")
            .property("padding", "0.75rem"),
        CssRule::new(".wb-column-title")
            .property("font-weight", "700")
            .property("text-transform", "uppercase")
            .property("font-size", "0.85rem")
            .property("letter-spacing", "0.04em")
            .property("color", "var(--bs-gray-400)"),
        // Fills the whole stretched column so an empty/short column still
        // has a `.wb-column-body` drop target `board_script` can hit.
        CssRule::new(".wb-column-body")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.6rem")
            .property("flex", "1 1 auto"),
        // Shown while a dragged card is over this column - see `board_script`.
        CssRule::new(".wb-column-body.wb-drop-target")
            .property("outline", "0.15rem dashed var(--bs-success-700)")
            .property("outline-offset", "-0.15rem"),
        CssRule::new(".wb-card")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.3rem")
            .property("background-color", "var(--bs-gray-800)")
            .property("padding", "0.6rem")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.4rem"),
        // `.wb-card` is shared with the (non-draggable) comment list.
        CssRule::new(".wb-column-body .wb-card").property("cursor", "grab"),
        CssRule::new(".wb-card-key")
            .property("font-size", "0.75rem")
            .property("color", "var(--bs-gray-500)"),
        CssRule::new(".wb-card-title")
            .property("font-size", "0.95rem")
            .property("color", "var(--bs-gray-100)")
            .property("text-decoration", "none"),
        CssRule::new(".wb-card-meta")
            .property("display", "flex")
            .property("gap", "0.5rem")
            .property("font-size", "0.75rem")
            .property("color", "var(--bs-gray-500)"),
    ]
}

/// The "+" trigger and the "new project" modal; only `.modal-center`'s
/// positioning needs a local override, the rest comes from the shared shell.
fn modal_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".home-header")
            .property("flex-direction", "row")
            .property("align-items", "center")
            .property("justify-content", "space-between"),
        CssRule::new(".wb-add-button")
            .property("width", "2.2rem")
            .property("height", "2.2rem")
            .property("border-radius", "50%")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("background-color", "var(--bs-gray-800)")
            .property("color", "var(--bs-gray-100)")
            .property("font-size", "1.3rem")
            .property("line-height", "1")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "center")
            .property("cursor", "pointer")
            .property("padding", "0")
            .child(CssRule::new("&:hover").property("background-color", "var(--bs-gray-700)")),
        // Stays centered (not off-screen) while hidden, so needs its own
        // `pointer-events: none` or it swallows clicks meant for the board.
        CssRule::new(".modal-center")
            .property("top", "50%")
            .property("left", "50%")
            .property("transform", "translate(-50%, -50%) scale(0.96)")
            .property("pointer-events", "none")
            .child(
                CssRule::new("&.show")
                    .property("transform", "translate(-50%, -50%) scale(1)")
                    .property("pointer-events", "auto"),
            ),
        CssRule::new(".wb-modal-header")
            .property("display", "flex")
            .property("align-items", "center")
            .property("justify-content", "space-between"),
        CssRule::new(".wb-modal-close")
            .property("border", "none")
            .property("background", "none")
            .property("color", "var(--bs-gray-400)")
            .property("font-size", "1.5rem")
            .property("line-height", "1")
            .property("cursor", "pointer")
            .property("padding", "0")
            .child(CssRule::new("&:hover").property("color", "var(--bs-gray-100)")),
    ]
}

/// The issue detail page's dependency lists (`blocks`/`blocked by`/
/// `relates to`) and the add-link form under them.
fn link_rules() -> Vec<CssRule> {
    vec![
        CssRule::new(".wb-link-section").property("padding", "0.5rem 1rem 0"),
        CssRule::new(".wb-link-section-title")
            .property("font-size", "0.8rem")
            .property("font-weight", "600")
            .property("text-transform", "uppercase")
            .property("letter-spacing", "0.03em")
            .property("color", "var(--bs-gray-500)")
            .property("margin-bottom", "0.4rem"),
        CssRule::new(".wb-link-list")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.4rem"),
        CssRule::new(".wb-link-row")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.6rem")
            .property("padding", "0.4rem 0.6rem")
            .property("border", "0.1rem solid var(--bs-gray-700)")
            .property("border-radius", "0.3rem")
            .property("background-color", "var(--bs-gray-800)"),
        CssRule::new(".wb-link-title")
            .property("flex", "1 1 auto")
            .property("min-width", "0")
            .property("color", "var(--bs-gray-100)")
            .property("text-decoration", "none")
            .property("overflow", "hidden")
            .property("text-overflow", "ellipsis")
            .property("white-space", "nowrap")
            .child(CssRule::new("&:hover").property("text-decoration", "underline")),
        CssRule::new(".wb-link-status")
            .property("flex", "0 0 auto")
            .property("font-size", "0.75rem")
            .property("color", "var(--bs-gray-500)"),
        // Unwraps the form so the button itself, not its block parent, sizes.
        CssRule::new(".wb-link-remove-form").property("display", "contents"),
        CssRule::new(".wb-link-remove")
            .property("flex", "0 0 auto")
            .property("width", "auto")
            .property("border", "none")
            .property("background", "none")
            .property("color", "var(--bs-gray-500)")
            .property("font-size", "1.1rem")
            .property("line-height", "1")
            .property("cursor", "pointer")
            .property("padding", "0 0.2rem")
            .child(CssRule::new("&:hover").property("color", "var(--bs-gray-100)")),
    ]
}
