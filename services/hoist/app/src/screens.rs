//! The screens: the catalog and the settings. Each is a function of its data and of what to do when the user
//! asks for something, so it runs the same in the browser, in a WebView host and on the native renderer, and a
//! test can drive it with no host at all.

use std::rc::Rc;

use hoist_core::{Entry, InstallState, format_bytes};
use quench_ui::kit::switch;
use quench_ui::prelude::*;

/// What the user can ask the catalog for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Install(String),
    Update(String),
    Open(String),
    UpdateAll,
    /// Show the page of one program.
    Details(String),
}

/// The callback a screen reports to.
pub type Act = Rc<dyn Fn(Action)>;

/// The words for where a program stands.
#[must_use]
pub fn status(entry: &Entry) -> String {
    match (entry.state(), &entry.installed) {
        (InstallState::UpdateAvailable, Some(i)) => {
            format!(
                "Update available: {} (installed {})",
                entry.latest.version_name, i.version_name
            )
        }
        (InstallState::UpToDate, _) => format!("Up to date: {}", entry.latest.version_name),
        _ => format!("Not installed: {}", entry.latest.version_name),
    }
}

/// The text of a row's button.
#[must_use]
pub const fn button_label(state: InstallState) -> &'static str {
    match state {
        InstallState::NotInstalled => "Install",
        InstallState::UpdateAvailable => "Update",
        InstallState::UpToDate => "Open",
    }
}

fn action_for(entry: &Entry) -> Action {
    let program = entry.latest.program.clone();
    match entry.state() {
        InstallState::NotInstalled => Action::Install(program),
        InstallState::UpdateAvailable => Action::Update(program),
        InstallState::UpToDate => Action::Open(program),
    }
}

/// Whether `entry` is what a search for `query` asks for: the query is in its title or its program name, in any case.
fn matches(entry: &Entry, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty()
        || entry.latest.title().to_lowercase().contains(&q)
        || entry.latest.program.to_lowercase().contains(&q)
}

/// The line under a row's title: the version and the size, and what is installed when that differs.
fn meta(entry: &Entry) -> String {
    let latest = &entry.latest;
    let base = format!(
        "v{} \u{b7} {}",
        latest.version_name,
        format_bytes(latest.size_bytes)
    );
    match (&entry.installed, entry.state()) {
        (Some(i), InstallState::UpdateAvailable) => {
            format!("{base} \u{b7} installed v{}", i.version_name)
        }
        _ => base,
    }
}

/// One row: tapping the words opens the program's page; the button does what the program needs (install, update,
/// open).
fn row(entry: Entry, act: &Act) -> View {
    let (act_main, act_button) = (Rc::clone(act), Rc::clone(act));
    let (program, action) = (entry.latest.program.clone(), action_for(&entry));
    let title = entry.latest.title().to_string();
    li().class("hoist-row")
        .child(
            button()
                .attr("type", "button")
                .class("hoist-row-main")
                .attr("aria-label", format!("Details {title}"))
                .child(crate::icons::download("hoist-row-icon"))
                .child(
                    div()
                        .class("hoist-row-text")
                        .child(span().class("hoist-row-title").text(title.clone()))
                        .child(
                            span()
                                .class("hoist-row-package")
                                .text(entry.latest.program.clone()),
                        )
                        .child(span().class("hoist-row-meta").text(meta(&entry))),
                )
                .on_click(move || act_main(Action::Details(program.clone()))),
        )
        .child(
            button()
                .attr("type", "button")
                .class("hoist-action")
                .attr(
                    "aria-label",
                    format!("{} {title}", button_label(entry.state())),
                )
                .text(button_label(entry.state()))
                .on_click(move || act_button(action.clone())),
        )
        .into_view()
}

/// An app that is on the device, whatever put it there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceApp {
    pub program: String,
    pub label: String,
    pub version_name: String,
}

/// The apps of the device that the registry does not list and that the search matches.
fn others(device: &[DeviceApp], entries: &[Entry], query: &str) -> Vec<DeviceApp> {
    let query = query.trim().to_lowercase();
    device
        .iter()
        .filter(|d| !entries.iter().any(|e| e.latest.program == d.program))
        .filter(|d| {
            query.is_empty()
                || d.label.to_lowercase().contains(&query)
                || d.program.to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
}

fn device_row(app: DeviceApp, act: &Act) -> View {
    let (program, act) = (app.program.clone(), Rc::clone(act));
    let label = if app.label.is_empty() {
        app.program.clone()
    } else {
        app.label.clone()
    };
    li().class("hoist-row")
        .child(
            div()
                .class("hoist-row-main")
                .child(crate::icons::apps("hoist-row-icon"))
                .child(
                    div()
                        .class("hoist-row-text")
                        .child(span().class("hoist-row-title").text(label.clone()))
                        .child(span().class("hoist-row-package").text(app.program.clone()))
                        .child(span().class("hoist-row-meta").text(
                            if app.version_name.is_empty() {
                                String::new()
                            } else {
                                format!("v{}", app.version_name)
                            },
                        )),
                ),
        )
        .child(
            button()
                .attr("type", "button")
                .class("hoist-action")
                .attr("aria-label", format!("Open {label}"))
                .text("Open")
                .on_click(move || act(Action::Open(program.clone()))),
        )
        .into_view()
}

/// The catalog: a search field, one row per program of the registry that matches it, and below them, under their
/// own heading, the other apps of the device that match it. The rows are made again when what they show changes (an
/// install makes "Update" into "Open").
#[must_use]
pub fn catalog(
    entries: &Signal<Vec<Entry>>,
    device: &Signal<Vec<DeviceApp>>,
    act: &Act,
    search: &Signal<String>,
) -> View {
    let (shown, query, typed) = (entries.clone(), search.clone(), search.clone());
    let (rest, rest_entries, rest_query, any_device, any_entries, any_query) = (
        device.clone(),
        entries.clone(),
        search.clone(),
        device.clone(),
        entries.clone(),
        search.clone(),
    );
    let act_device = Rc::clone(act);
    let act_rows = Rc::clone(act);
    div()
        .attr("id", "catalog")
        .class("hoist-screen")
        .child(
            div()
                .class("hoist-searchbar")
                .child(crate::icons::search("hoist-search-icon"))
                .child(
                    input()
                        .attr("type", "text")
                        .attr("id", "search")
                        .attr("placeholder", "Search apps")
                        .attr("aria-label", "Search apps")
                        .class("hoist-search")
                        .on_input(move |v| typed.set(v.to_string())),
                ),
        )
        .child(
            div()
                .class("hoist-scroll")
                .child(ul().class("hoist-list").child(View::keyed(
                    move || {
                        shown
                            .get()
                            .into_iter()
                            .filter(|e| matches(e, &query.get()))
                            .collect::<Vec<_>>()
                    },
                    |e: &Entry| {
                        format!(
                            "{}@{}@{}",
                            e.latest.program,
                            e.latest.version_code,
                            e.installed.as_ref().map_or(0, |i| i.version_code)
                        )
                    },
                    move |entry: Entry| row(entry, &act_rows),
                )))
                .child(View::show(
                    move || {
                        !others(&any_device.get(), &any_entries.get(), &any_query.get()).is_empty()
                    },
                    move || {
                        let (rest, entries, query, act) = (
                            rest.clone(),
                            rest_entries.clone(),
                            rest_query.clone(),
                            Rc::clone(&act_device),
                        );
                        div()
                            .attr("id", "device-apps")
                            .child(h3().text("Installed apps"))
                            .child(ul().class("hoist-list").child(View::keyed(
                                move || others(&rest.get(), &entries.get(), &query.get()),
                                |d: &DeviceApp| format!("{}@{}", d.program, d.version_name),
                                move |app: DeviceApp| device_row(app, &act),
                            )))
                            .into_view()
                    },
                )),
        )
        .into_view()
}

/// What has an update: a button for all of them, and a row for each.
#[must_use]
pub fn updates(entries: &Signal<Vec<Entry>>, act: &Act) -> View {
    let (due, any, act_rows) = (entries.clone(), entries.clone(), Rc::clone(act));
    let act_all = Rc::clone(act);
    div()
        .attr("id", "updates")
        .class("hoist-screen hoist-scrolls")
        .child(View::show(
            move || {
                any.get()
                    .iter()
                    .any(|e| e.state() == InstallState::UpdateAvailable)
            },
            move || {
                let act = Rc::clone(&act_all);
                button()
                    .attr("id", "update-all")
                    .attr("type", "button")
                    .class("hoist-primary")
                    .text("Update all")
                    .on_click(move || act(Action::UpdateAll))
                    .into_view()
            },
        ))
        .child(p().attr("id", "updates-note").class("hoist-note").text_fn({
            let entries = entries.clone();
            move || {
                let n = entries
                    .get()
                    .iter()
                    .filter(|e| e.state() == InstallState::UpdateAvailable)
                    .count();
                match n {
                    0 => "Everything is up to date".to_string(),
                    1 => "1 app has an update".to_string(),
                    n => format!("{n} apps have updates"),
                }
            }
        }))
        .child(ul().class("hoist-list").child(View::keyed(
            move || {
                due.get()
                    .into_iter()
                    .filter(|e| e.state() == InstallState::UpdateAvailable)
                    .collect::<Vec<_>>()
            },
            |e: &Entry| format!("{}@{}", e.latest.program, e.latest.version_code),
            move |entry: Entry| row(entry, &act_rows),
        )))
        .into_view()
}

/// A long hash in groups of sixteen, so it can wrap on a narrow screen.
fn spaced(hash: &str) -> String {
    hash.as_bytes()
        .chunks(16)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The page of one program: what the registry says about its latest build, where it stands on this machine, the
/// button for what to do about it. (The way back is the app bar's.)
#[must_use]
pub fn detail(entry: &Entry, act: &Act) -> View {
    let latest = &entry.latest;
    let row = |name: &str, value: String| {
        div()
            .class("hoist-fact")
            .child(span().class("hoist-fact-name").text(name.to_string()))
            .child(span().class("hoist-fact-value").text(value))
    };
    let action = action_for(entry);
    let act = Rc::clone(act);
    let published = latest
        .created_at
        .split('T')
        .next()
        .unwrap_or(&latest.created_at);
    let mut page = div()
        .attr("id", "detail")
        .class("hoist-screen hoist-scrolls")
        .child(h2().text(latest.title().to_string()))
        .child(p().class("hoist-package").text(latest.program.clone()))
        .child(row(
            "Version",
            format!("{} ({})", latest.version_name, latest.version_code),
        ))
        .child(row("Size", format_bytes(latest.size_bytes)))
        .child(row(
            "Format",
            [
                latest.platform.as_str(),
                latest.arch.as_deref().unwrap_or("any"),
                latest.format.as_str(),
            ]
            .join(" · "),
        ))
        .child(row(
            "Published by",
            format!("{} on {published}", latest.uploaded_by),
        ))
        .child(row("SHA-256", spaced(&latest.sha256)));
    if let Some(installed) = &entry.installed {
        page = page.child(row(
            "Installed",
            format!("{} ({})", installed.version_name, installed.version_code),
        ));
    }
    if let Some(min) = latest.min_sdk_version {
        let built = latest
            .target_sdk_version
            .map_or_else(String::new, |t| format!(", built for {t}"));
        page = page.child(row("Android", format!("from API {min}{built}")));
    }
    if !latest.permissions.is_empty() {
        page = page.child(h3().text("Permissions")).child(
            div().class("hoist-chips").children(
                latest
                    .permissions
                    .iter()
                    .map(|p| span().class("hoist-chip").text(p.clone()).into_view()),
            ),
        );
    }
    // The button first, as in the store it follows: what to do, then where the program stands.
    page.child(
        button()
            .attr("type", "button")
            .attr("id", "detail-action")
            .class("hoist-primary")
            .text(button_label(entry.state()))
            .on_click(move || act(action.clone())),
    )
    .child(
        p().attr("id", "detail-status")
            .class("hoist-note")
            .text(status(entry)),
    )
    .into_view()
}

/// What the settings screen edits.
#[derive(Clone)]
pub struct Settings {
    pub registry_url: quench_ui::form::Field,
    pub auto_check: Signal<bool>,
}

impl Settings {
    #[must_use]
    pub fn new(registry_url: &str, auto_check: bool) -> Self {
        Self {
            registry_url: quench_ui::form::Field::new("registry", registry_url).rule(
                quench_ui::form::rules::check(
                    |v| v.starts_with("https://") || v.starts_with("http://"),
                    "The address starts with https://",
                ),
            ),
            auto_check: create_signal(auto_check),
        }
    }
}

/// One line of the settings: a title, a few words under it, and what goes at the right.
fn setting(id: &str, title: &str, note: &str, control: Option<Element>) -> Element {
    div()
        .attr("id", id)
        .class("hoist-setting")
        .child(
            div()
                .class("hoist-setting-text")
                .child(span().class("hoist-setting-title").text(title.to_string()))
                .child(
                    (!note.is_empty())
                        .then(|| span().class("hoist-setting-note").text(note.to_string())),
                ),
        )
        .child(control)
}

/// The settings: the server, whether to look for updates in the background, a way to look now, signing out, and the
/// version. Nothing needs saving: `on_change` is told when the server address (once it is a valid one) or the
/// switch changes.
#[must_use]
pub fn settings(
    settings: &Settings,
    version: &str,
    check_now: impl Fn() + 'static,
    sign_out: impl Fn() + 'static,
) -> View {
    let server = settings.registry_url.clone();
    let shown = server.clone();
    div()
        .attr("id", "settings")
        .class("hoist-screen hoist-scrolls")
        .child(
            div()
                .class("hoist-setting hoist-setting-column")
                .child(span().class("hoist-setting-title").text("Server"))
                .child(
                    span()
                        .attr("id", "server")
                        .class("hoist-setting-note")
                        .text_fn(move || shown.value()),
                ),
        )
        .child(div().class("hoist-rule"))
        .child(setting(
            "auto-check",
            "Check for updates in the background",
            "Every few hours, looks for updates and sends a notification when apps can be updated (on a phone even when Hoist is closed). Never installs anything on its own.",
            Some(switch("", &settings.auto_check).attr("aria-label", "Check for updates in the background")),
        ))
        .child(
            button()
                .attr("type", "button")
                .attr("id", "check-now")
                .class("hoist-setting hoist-setting-button")
                .on_click(check_now)
                .child(
                    div()
                        .class("hoist-setting-text")
                        .child(span().class("hoist-setting-title").text("Check now"))
                        .child(span().class("hoist-setting-note").text("Run the update check once, right now")),
                )
                .child(crate::icons::refresh("hoist-setting-icon")),
        )
        .child(div().class("hoist-rule"))
        .child(
            button()
                .attr("type", "button")
                .attr("id", "logout")
                .class("hoist-setting hoist-setting-button")
                .on_click(sign_out)
                .child(
                    div()
                        .class("hoist-setting-text")
                        .child(span().class("hoist-setting-title").text("Sign out"))
                        .child(span().class("hoist-setting-note").text("Forget this session on this device")),
                )
                .child(crate::icons::sign_out("hoist-setting-icon")),
        )
        .child(setting("version", "Version", version, None))
        .into_view()
}

/// A tab of the bottom bar: its name and the function that draws its icon.
pub type Tab = (&'static str, fn(&str) -> Element);

/// The bar along the bottom: a drawn icon over each name, the one of `selected` on a pill. (The kit's bar takes
/// symbol characters, which are thin and differ from font to font.)
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
