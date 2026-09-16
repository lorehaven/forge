//! Conveyor's own slide-out nav drawer - quench-web's `NavPanelBuilder` only
//! renders locale/theme selects and has a real init-script bug, so this hand-rolls one.

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
