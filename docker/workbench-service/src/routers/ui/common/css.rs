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

/// `quench-web`'s shared theme sets no `box-sizing`, so it defaults to the
/// browser's own `content-box` - a `.wb-form`'s `padding: 1rem` (below) then
/// adds on top of the shared `form { width: 100% }` rule instead of being cut
/// out of it, rendering every form 2rem wider than its panel and pushing
/// anything flush with its right edge (a right-aligned `.wb-submit` button,
/// the last column in `.wb-form-row`) out past the panel's actual border.
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
        // Block, not inline - every label sits on its own line above the
        // control it labels (`wb-field-row`'s fields, and the bare
        // label+control pairs `wb-form-row` wraps) rather than to its left,
        // which is what frees up a field's full width for its control - the
        // assignee picker's select-plus-button in particular needs it to
        // stay on screen instead of overflowing the row.
        CssRule::new(".wb-form label")
            .property("display", "block")
            .property("font-size", "0.85rem")
            .property("color", "var(--bs-gray-400)")
            .property("margin-bottom", "0.3rem"),
        CssRule::new(".wb-field-row")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "0.3rem"),
        // The assignee picker's select + "assign to me" shortcut, sharing
        // the field row's control column rather than each taking their own.
        CssRule::new(".wb-field-control")
            .property("display", "flex")
            .property("flex", "1 1 auto")
            .property("min-width", "0")
            .property("flex-wrap", "wrap")
            .property("align-items", "center")
            .property("gap", "0.5rem"),
        // Lets the select shrink below its content's width (e.g. a long
        // username) instead of forcing `.wb-field-control` past its own
        // container - the "assign to me" button next to it stays fixed-size
        // (`flex: 0 0 auto` above) and wraps under it if there truly isn't
        // room for both on one line.
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
        // quench-web's shared theme styles `input`/`select` dark but has no
        // rule for `textarea` at all, so ours would otherwise fall back to
        // the browser's default white one - matched to `input`'s own rule
        // (`quench-web/framework/styles/common/elements.rs`) rather than
        // introducing a different look for one field in the same form.
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
        // Every `.wb-form`'s own submit button - small and right-aligned
        // rather than the shared `button { display: flex }` rule's full-width
        // block-level default (`.wb-form`'s `display: flex; flex-direction:
        // column` stretches children across the cross axis unless a child
        // opts out via `align-self`, which also switches its own width back
        // to content-based since it is no longer stretched).
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
        // The shared shell normally lets `.content-inner` grow past
        // `.content`'s box and relies on `.content`'s own `overflow-y: auto`
        // (quench-web's shared shell rule) to scroll the whole page - right
        // for an ordinary page, wrong here: the columns already own their
        // scrolling (`.wb-column-body` below), so the ancestor chain instead
        // needs to respect `.content`'s real height and hand `.wb-board`
        // whatever's left, rather than growing past it and leaving two
        // separate scrollbars. Scoped to `:has(.wb-board)` so every other
        // page (an ordinary list, a form) keeps the shared shell's normal
        // "grow and let the page scroll" behaviour.
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
        // The board itself is the one scrollbar, in both directions: sideways
        // once there are more columns than fit (each auto-generated column's
        // own `minmax(16rem, ...)` floor below is what forces that rather
        // than squeezing them), and downward once the tallest column's cards
        // do not fit in the space above.
        //
        // Grid, not flex: with flex, a scrolling flex container
        // (`overflow: auto`, needed for the horizontal scroll above)
        // suppresses a stretched item's content-based automatic minimum size
        // - that is the CSS spec's own rule, not a bug here - so a `flex`
        // column would stop at the row's height and let its own cards spill
        // out below its background instead of growing to contain them. Grid
        // has no such rule: an implicit row's height is measured from its
        // items' content first, so `align-items: stretch` (grid's default)
        // correctly grows every column to match the tallest one's actual
        // content height.
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
        // `flex: 1 1 auto` so this fills the whole (grid-stretched) column,
        // not just the height its own cards need - otherwise a shorter or
        // empty column leaves dead space below/instead of it that is not
        // `.wb-column-body`, and a card dropped there misses the drop target
        // `board_script` listens for entirely.
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
        // Only the board's own cards are draggable - `.wb-card` is shared with
        // the issue page's comment list, which is not.
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

/// The "+" trigger next to a page title and the "new project" modal it opens.
///
/// `.modal-overlay`/`.modal-center`/`.modal-content` are quench-web's own
/// (baked into every page via `AppShellBuilder`, see `common/mod.rs`) - only
/// `.modal-center`'s positioning needs a local override, since the shared
/// rule ships with no top/left of its own (`inset: auto` alone does not
/// center it), and the panel chrome (background/border/shadow/show
/// transition) already comes free from that shared rule otherwise.
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
        // The shared rule hides `.modal-center` with opacity alone, relying
        // on `.modal-side`'s sibling variant moving fully off-screen
        // (`translateX`) to also take it out of the hit-testing path. This
        // override keeps the modal centered even while hidden (just scaled
        // down and transparent) rather than off-screen, so it needs its own
        // `pointer-events: none` - without it, an invisible modal still sits
        // on top of the board and swallows clicks and drags meant for
        // whatever is under it.
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
        // Unwraps the delete form so `.wb-link-remove` itself, not its
        // block-level form parent, is the flex item `.wb-link-row` sizes -
        // otherwise the shared `button { display: flex }` rule's block-level
        // sizing fills the form's full-width box instead of shrinking to fit
        // the "×" glyph.
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
