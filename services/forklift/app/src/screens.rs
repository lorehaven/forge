//! The dashboard and the settings, as views over plain data.

use std::rc::Rc;

use forklift_core::{Folder, RestoreView, Snapshot, bytes, clock};
use quench_ui::kit::switch;
use quench_ui::prelude::*;

/// Where a backup is, in words and a bar: the headline, how far along, what is being sent, per-category counts and
/// the button.
#[must_use]
pub fn dashboard(shot: &Signal<Snapshot>, offset_minutes: i32, back_up: Rc<dyn Fn()>) -> View {
    let (head, bar, sub, rows, button_state) = (
        shot.clone(),
        shot.clone(),
        shot.clone(),
        shot.clone(),
        shot.clone(),
    );
    div()
        .attr("id", "dashboard")
        .class("forklift-screen forklift-scrolls")
        .child(
            div()
                .class("forklift-card")
                .child(
                    h2().attr("id", "headline")
                        .class("forklift-headline")
                        .text_fn(move || head.get().headline()),
                )
                .child(div().class("forklift-bar").child(
                    div().class("forklift-bar-fill").attr_fn("style", move || {
                        format!("width:{:.0}%", bar.get().fraction() * 100.0)
                    }),
                ))
                // Three lines that are always there, each too short to wrap: the card keeps its height whatever the file
                // names are (a line with nothing to say holds its place with a no-break space).
                .child(sub_line("sending-line", &sub, move |s| {
                    match s.sending.first() {
                        // A scan under way says how far it is, in place of a file's name.
                        None if s.looked_at > 0 => {
                            format!("Looking at your files \u{b7} {}", s.looked_at)
                        }
                        None => String::new(),
                        Some(name) => {
                            let more = s.totals.uploading.saturating_sub(1);
                            if more == 0 {
                                format!("Sending {}", shorten(name, 30))
                            } else {
                                format!("Sending {} and {more} more", shorten(name, 22))
                            }
                        }
                    }
                }))
                .child(sub_line("figures-line", &sub, move |s| {
                    let mut parts = Vec::new();
                    if let Some(at) = s.checked_ms {
                        parts.push(format!("Last checked {}", clock(at, offset_minutes)));
                    }
                    if s.totals.waiting_bytes > 0 {
                        parts.push(format!("{} to go", bytes(s.totals.waiting_bytes)));
                    }
                    parts.join(" \u{b7} ")
                }))
                .child(sub_line("storage-line", &sub, |s| {
                    s.quota
                        .map(|(used, quota)| format!("Storage {} of {}", bytes(used), bytes(quota)))
                        .unwrap_or_default()
                })),
        )
        .child(
            button()
                .attr("type", "button")
                .attr("id", "back-up-now")
                .class("forklift-primary")
                .text_fn(move || {
                    if button_state.get().working {
                        "Working\u{2026}"
                    } else {
                        "Back up now"
                    }
                    .to_string()
                })
                .on_click(move || back_up()),
        )
        .child(View::dynamic(move || {
            let list = rows.get().rows;
            ul().class("forklift-list")
                .children(list.into_iter().map(|r| {
                    let t = r.totals;
                    li().attr("id", format!("row-{}", r.key.replace(':', "-")))
                        .class("forklift-row")
                        .child(
                            div()
                                .class("forklift-row-main")
                                .child(crate::icons::backup("forklift-row-icon"))
                                .child(
                                    div()
                                        .class("forklift-row-text")
                                        .child(span().class("forklift-row-title").text(r.label))
                                        .child(span().class("forklift-row-meta").text(format!(
                                            "{} backed up \u{b7} {} waiting{}",
                                            t.done,
                                            t.pending + t.uploading,
                                            if t.failed > 0 {
                                                format!(" \u{b7} {} failed", t.failed)
                                            } else {
                                                String::new()
                                            }
                                        ))),
                                ),
                        )
                        .into_view()
                }))
                .into_view()
        }))
        .child(p().class("forklift-note").text_fn({
            let f = shot.clone();
            move || {
                let n = f.get().totals.too_large;
                if n == 0 {
                    String::new()
                } else {
                    format!("{n} too large for the server, left out")
                }
            }
        }))
        .into_view()
}

/// At most `max` characters of `name`, with an ellipsis where it was cut.
#[must_use]
pub fn shorten(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let kept: String = name.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}\u{2026}")
}

/// One line of the dashboard card: its text from the snapshot, or a no-break space that holds the line's height.
fn sub_line(
    id: &str,
    shot: &Signal<Snapshot>,
    text: impl Fn(&Snapshot) -> String + 'static,
) -> Element {
    let shot = shot.clone();
    p().attr("id", id.to_string())
        .class("forklift-sub")
        .text_fn(move || {
            let said = text(&shot.get());
            if said.is_empty() {
                "\u{a0}".to_string()
            } else {
                said
            }
        })
}

/// What the settings screen edits.
#[derive(Clone)]
pub struct Controls {
    pub auto: Signal<bool>,
    pub photos: Signal<bool>,
    pub videos: Signal<bool>,
    pub videos_wifi_only: Signal<bool>,
    pub charging_for_large: Signal<bool>,
    pub folders: Signal<Vec<Folder>>,
}

fn setting(id: &str, title: &str, note: &str, control: Option<Element>) -> Element {
    div()
        .attr("id", id)
        .class("forklift-setting")
        .child(
            div()
                .class("forklift-setting-text")
                .child(
                    span()
                        .class("forklift-setting-title")
                        .text(title.to_string()),
                )
                .child(
                    (!note.is_empty())
                        .then(|| span().class("forklift-setting-note").text(note.to_string())),
                ),
        )
        .child(control)
}

/// What the settings screen shows besides the switches.
pub struct About<'a> {
    /// Whether this device has a media library and a battery to wait for (a phone does, a computer does not).
    pub phone: bool,
    pub server: &'a str,
    pub storage: &'a str,
    pub version: &'a str,
}

/// What the buttons of the settings screen do.
pub struct Actions {
    pub add_folder: Rc<dyn Fn()>,
    pub remove_folder: Rc<dyn Fn(String)>,
    pub change_storage: Rc<dyn Fn()>,
    pub restore: Rc<dyn Fn()>,
    pub sign_out: Rc<dyn Fn()>,
}

/// The settings: what to back up, the conditions, the folders, the storage, signing out.
#[must_use]
pub fn settings(about: &About, c: &Controls, actions: Actions) -> View {
    let (phone, server, storage, version) =
        (about.phone, about.server, about.storage, about.version);
    let Actions {
        add_folder,
        remove_folder,
        change_storage,
        restore,
        sign_out,
    } = actions;
    let toggle = |id: &str, title: &str, note: &str, s: &Signal<bool>| {
        setting(
            id,
            title,
            note,
            Some(switch("", s).attr("aria-label", title.to_string())),
        )
    };
    let folders = c.folders.clone();
    let wifi = c.folders.clone();
    div()
        .attr("id", "settings")
        .class("forklift-screen forklift-scrolls")
        .child(
            div()
                .class("forklift-setting forklift-setting-column")
                .child(span().class("forklift-setting-title").text("Server"))
                .child(
                    span()
                        .attr("id", "server")
                        .class("forklift-setting-note")
                        .text(server.to_string()),
                ),
        )
        .child(
            button()
                .attr("type", "button")
                .attr("id", "storage")
                .class("forklift-setting forklift-setting-button")
                .on_click(move || change_storage())
                .child(
                    div()
                        .class("forklift-setting-text")
                        .child(span().class("forklift-setting-title").text("Storage"))
                        .child(
                            span()
                                .attr("id", "storage-name")
                                .class("forklift-setting-note")
                                .text(storage.to_string()),
                        ),
                )
                .child(span().class("forklift-link").text("Change")),
        )
        .child(div().class("forklift-rule"))
        .child(toggle(
            "auto",
            "Back up automatically",
            "Looks for new files as they appear and every so often, even when Forklift is closed",
            &c.auto,
        ))
        // The media library and the battery are a phone's: a computer backs up its folders.
        .child(phone.then(|| toggle("photos", "Photos", "", &c.photos)))
        .child(phone.then(|| toggle("videos", "Videos", "", &c.videos)))
        .child(phone.then(|| {
            toggle(
                "videos-wifi",
                "Videos on Wi-Fi only",
                "",
                &c.videos_wifi_only,
            )
        }))
        .child(phone.then(|| {
            toggle(
                "charging",
                "Large files only while charging",
                "Files over 200 MB wait for the charger",
                &c.charging_for_large,
            )
        }))
        .child(div().class("forklift-rule"))
        .child(
            div()
                .class("forklift-section-head")
                .child(h3().text("Folders"))
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "add-folder")
                        .class("forklift-link")
                        .text("Add")
                        .on_click(move || add_folder()),
                ),
        )
        .child(View::dynamic(move || {
            let list = folders.get();
            if list.is_empty() {
                return p()
                    .class("forklift-note")
                    .text("No folders yet")
                    .into_view();
            }
            let remove = Rc::clone(&remove_folder);
            let wifi = wifi.clone();
            ul().class("forklift-list")
                .children(list.into_iter().map(move |f| {
                    let (slug, remove) = (f.slug.clone(), Rc::clone(&remove));
                    let (toggle_slug, toggle) = (f.slug.clone(), wifi.clone());
                    let network = if f.wifi_only {
                        "Wi-Fi only"
                    } else {
                        "Any network"
                    };
                    li().attr("id", format!("folder-{}", f.slug))
                        .class("forklift-row")
                        .child(
                            div()
                                .class("forklift-row-main")
                                .child(crate::icons::folder("forklift-row-icon"))
                                .child(span().class("forklift-row-title").text(f.name)),
                        )
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("id", format!("wifi-{}", f.slug))
                                .class("forklift-link")
                                .text(network)
                                .on_click(move || {
                                    let mut list = toggle.get();
                                    if let Some(f) = list.iter_mut().find(|f| f.slug == toggle_slug)
                                    {
                                        f.wifi_only = !f.wifi_only;
                                    }
                                    toggle.set(list);
                                }),
                        )
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("aria-label", "Remove folder")
                                .class("q-iconbtn")
                                .child(crate::icons::close("q-icon"))
                                .on_click(move || remove(slug.clone())),
                        )
                        .into_view()
                }))
                .into_view()
        }))
        .child(div().class("forklift-rule"))
        .child(
            button()
                .attr("type", "button")
                .attr("id", "restore")
                .class("forklift-setting forklift-setting-button")
                .on_click(move || restore())
                .child(
                    div()
                        .class("forklift-setting-text")
                        .child(
                            span()
                                .class("forklift-setting-title")
                                .text("Restore from backup"),
                        )
                        .child(
                            span()
                                .class("forklift-setting-note")
                                .text("Bring back what this device lacks"),
                        ),
                )
                .child(crate::icons::refresh("forklift-setting-icon")),
        )
        .child(div().class("forklift-rule"))
        .child(
            button()
                .attr("type", "button")
                .attr("id", "logout")
                .class("forklift-setting forklift-setting-button")
                .on_click(move || sign_out())
                .child(
                    div()
                        .class("forklift-setting-text")
                        .child(span().class("forklift-setting-title").text("Sign out"))
                        .child(
                            span()
                                .class("forklift-setting-note")
                                .text("Forget this session on this device"),
                        ),
                )
                .child(crate::icons::sign_out("forklift-setting-icon")),
        )
        .child(setting("version", "Version", version, None))
        .into_view()
}

/// A tab of the bottom bar.
pub type Tab = (&'static str, fn(&str) -> Element);

/// The bar along the bottom: a drawn icon over each name, the one of `selected` on a pill.
#[must_use]
pub fn bottom_bar(items: Vec<Tab>, selected: &Signal<usize>) -> Element {
    let buttons = items.into_iter().enumerate().map(|(i, (label, icon))| {
        let (current, choose) = (selected.clone(), selected.clone());
        button()
            .attr("type", "button")
            .class("q-bottomnav-item")
            .attr_opt("aria-current", move || {
                (current.get() == i).then(|| "page".to_string())
            })
            .child(
                span()
                    .class("q-bottomnav-glyph")
                    .attr("aria-hidden", "true")
                    .child(icon("q-icon")),
            )
            .child(span().class("q-bottomnav-label").text(label))
            .on_click(move || choose.set(i))
            .into_view()
    });
    nav()
        .class("q-bottomnav")
        .attr("aria-label", "Main")
        .children(buttons)
}

/// What the restore dialog says for each state, and which buttons it has.
#[must_use]
pub fn restore_words(view: &RestoreView) -> String {
    match view {
        RestoreView::Idle => String::new(),
        RestoreView::Planning => "Looking at what is stored\u{2026}".into(),
        RestoreView::Plan(s) if s.files == 0 => {
            format!(
                "Nothing to restore: this device has the {} stored files that have a place here.",
                s.present
            )
        }
        RestoreView::Plan(s) => {
            let mut text = format!(
                "Restore {} {} ({})? {} already on this device.",
                s.files,
                if s.files == 1 { "file" } else { "files" },
                bytes(s.bytes),
                s.present
            );
            if s.no_target > 0 {
                text.push_str(&format!(
                    " {} are left out: their folder is not part of the backup any more.",
                    s.no_target
                ));
            }
            text
        }
        RestoreView::Running(p) => {
            let mut text = format!("Restoring: {} of {} files", p.done + p.failed, p.total);
            if p.failed > 0 {
                text.push_str(&format!(", {} could not be restored", p.failed));
            }
            text
        }
        RestoreView::Done(p) if p.total == 0 => "Nothing was restored.".into(),
        RestoreView::Done(p) => {
            let mut text = format!("Restored {} of {} files.", p.done, p.total);
            if p.failed > 0 {
                text.push_str(&format!(" {} could not be restored.", p.failed));
            }
            if p.done + p.failed < p.total {
                text.push_str(" The rest was left.");
            }
            text
        }
        RestoreView::Failed(why) => why.clone(),
    }
}

/// Where the storage chooser is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Chooser {
    #[default]
    Hidden,
    /// Asking the server.
    Loading,
    /// The storages the account may use.
    Choose(Vec<forklift_core::StorageInfo>),
    Failed(String),
}

/// The chooser of the storage the backup goes to: the ones this account may use, as rows to press. Shown before
/// anything is sent, and again when the person wants to change it.
#[must_use]
pub fn storage_screen(
    state: &Signal<Chooser>,
    current: &str,
    confirmed: bool,
    pick: Rc<dyn Fn(String)>,
    retry: Rc<dyn Fn()>,
    close: Rc<dyn Fn()>,
) -> View {
    let (state, current) = (state.clone(), current.to_string());
    View::dynamic(move || {
        let mut screen = div()
            .attr("id", "storage-chooser")
            .class("forklift-screen forklift-scrolls")
            .child(h2().text("Where should this device's backup go?"));
        match state.get() {
            Chooser::Hidden | Chooser::Loading => {
                screen = screen.child(p().class("forklift-note").text("Asking the server\u{2026}"));
            }
            Chooser::Failed(why) => {
                let again = Rc::clone(&retry);
                screen = screen
                    .child(
                        p().attr("id", "storage-problem")
                            .class("forklift-note")
                            .text(why),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "storage-retry")
                            .class("forklift-link")
                            .text("Try again")
                            .on_click(move || again()),
                    );
            }
            Chooser::Choose(list) if list.is_empty() => {
                let again = Rc::clone(&retry);
                screen = screen
                    .child(
                        p().attr("id", "storage-problem").class("forklift-note").text(
                            "This account has no storage it can use: ask whoever runs the server to add one, then try again.",
                        ),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "storage-retry")
                            .class("forklift-link")
                            .text("Try again")
                            .on_click(move || again()),
                    );
            }
            Chooser::Choose(list) => {
                let rows = list.into_iter().map(|s| {
                    let (name, pick) = (s.name.clone(), Rc::clone(&pick));
                    let meta = match (s.used, s.quota) {
                        (Some(used), Some(quota)) => format!("{} of {}", bytes(used), bytes(quota)),
                        (Some(used), None) => format!("{} used", bytes(used)),
                        _ => String::new(),
                    };
                    let marker = if s.name == current {
                        if confirmed {
                            "in use now"
                        } else {
                            "suggested for this device"
                        }
                    } else {
                        ""
                    };
                    let meta = [marker.to_string(), meta]
                        .into_iter()
                        .filter(|m| !m.is_empty())
                        .collect::<Vec<_>>()
                        .join(" \u{b7} ");
                    li().class("forklift-row")
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("id", format!("storage-{}", s.name))
                                .class("forklift-row-main")
                                .on_click(move || pick(name.clone()))
                                .child(crate::icons::backup("forklift-row-icon"))
                                .child(
                                    div()
                                        .class("forklift-row-text")
                                        .child(span().class("forklift-row-title").text(s.name))
                                        .child(span().class("forklift-row-meta").text(meta)),
                                ),
                        )
                        .into_view()
                });
                screen = screen.child(ul().class("forklift-list").children(rows));
                if confirmed {
                    screen = screen.child(
                        p().class("forklift-note")
                            .text("Choosing another one sends everything to it again; what is already stored stays where it is."),
                    );
                }
            }
        }
        if confirmed {
            let close = Rc::clone(&close);
            screen = screen.child(
                button()
                    .attr("type", "button")
                    .attr("id", "storage-cancel")
                    .class("forklift-link")
                    .text("Cancel")
                    .on_click(move || close()),
            );
        }
        screen.into_view()
    })
}
