//! Conveyor's own slide-out nav drawer.
//!
//! quench-web ships no reusable sidebar with page-nav entries - its only
//! drawer, `NavPanelBuilder`, renders exclusively the locale/theme
//! `<select>`s, has no extension point for anything else, and has a real
//! bug besides (its locale- and theme-select init scripts are joined into
//! one `<script>` body, both declaring `const select` - an immediate
//! `SyntaxError` at runtime). Palantir's `app/src/shell.rs` hand-rolls its
//! own drawer for exactly this reason, reusing only `nav_button()` (the
//! hamburger trigger, already wired to
//! `toggle_modal("modal-overlay", "modal-side", "show")`) and the
//! `.modal-overlay`/`.modal-side`/`.modal-content` CSS quench-web ships
//! pre-styled. This does the same, in conveyor's own crate - palantir is a
//! separate git repo and a separate `quench-web` registry dependency, so
//! there is no shared component to import.
//!
//! Unlike palantir's per-section, path-dependent entry list, this one is
//! two static links and carries no locale/theme select at all - so, unlike
//! `nav::panel(current_path)` there, it needs no per-request rebuild and no
//! current-path threading through every page. It is built once, as part of
//! the already-`LazyLock`'d header.

use crate::routers::ui::common::ui_path;
use quench_web::framework::dom::toggle_modal;
use quench_web::prelude::*;

struct Entry {
    /// Translation key - `ui_home_button` is reused as-is here, the header
    /// already uses it for the same destination.
    key: &'static str,
    icon: &'static str,
    href: &'static str,
}

const ENTRIES: &[Entry] = &[
    Entry {
        key: "ui_home_button",
        icon: "fa-diagram-project",
        href: "/home",
    },
    Entry {
        key: "ui_nav_credentials",
        icon: "fa-key",
        href: "/credentials",
    },
];

pub fn panel() -> Element {
    let toggle = toggle_modal("modal-overlay", "modal-side", "show");

    let mut entries = div().class("side-nav-bar");
    for entry in ENTRIES {
        entries = entries.child(
            a().attr("href", ui_path(entry.href))
                .class("side-nav-bar-entry")
                .child(i().class("fas").class(entry.icon))
                .child(span().attr("data-i18n", entry.key)),
        );
    }

    div()
        .child(div().class("modal-overlay").on_click(&toggle))
        .child(
            div()
                .class("modal-side")
                .child(div().class("modal-content").child(entries)),
        )
}
