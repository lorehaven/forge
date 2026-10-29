//! The app against the real services: sign in, the registry's catalog, downloads. The app does no I/O itself: it
//! asks for things through a `requests` function and is told what happened through `deliver` (both carry plain
//! data), so the host decides how to do the work (a worker thread, a test double) and where the answers come in.

use std::collections::BTreeMap;
use std::rc::Rc;

use hoist_core::{Artifact, Entry, Installed, build_entries};
use quench_ui::form::{Field, Form, rules};
use quench_ui::kit::{app_bar, dialog, icon_button, text_field};
use quench_ui::prelude::*;

use crate::screens::{Act, Action, Settings, catalog, detail, settings, updates};

/// What the app asks its host to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Login {
        server: String,
        username: String,
        password: String,
    },
    Catalog,
    Download(Artifact),
    /// Download a build, hand it to the system to install, and remember that it was.
    Install(Artifact),
    Logout,
    /// A page of the public catalogue (hoist-service).
    PublicPage(hoist_core::PublicQuery),
    /// The public catalogue's categories.
    PublicCategories,
    /// Download an app of the public catalogue: no token goes to the store it comes from.
    DownloadPublic(hoist_core::PublicApp),
    /// Download an app of the public catalogue, hand it to the system to install, and remember that it was.
    InstallPublic(hoist_core::PublicApp),
    /// Open an app that is installed (the host starts it).
    Launch(String),
    /// The user changed a setting: the host keeps it.
    Preferences {
        server: String,
        auto_check: bool,
    },
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// Signed in; the host keeps the refresh token.
    LoggedIn,
    LoginFailed(String),
    Catalog(Vec<Artifact>),
    CatalogFailed(String),
    /// A build was downloaded and verified, and is at `path`.
    Downloaded {
        program: String,
        path: String,
    },
    DownloadFailed(String),
    /// What the system says is installed (an Android host asks its package manager): merged into the ledger.
    Known(Vec<Installed>),
    /// What the public catalogue answered for `asked`.
    PublicPage {
        asked: hoist_core::PublicQuery,
        page: hoist_core::PublicPage,
    },
    Categories(Vec<hoist_core::Category>),
    /// The public catalogue could not be read (`asked` is the page that was wanted, if it was a page).
    PublicFailed {
        asked: Option<hoist_core::PublicQuery>,
        why: String,
    },
    /// The apps the device has (an Android host asks its package manager), for the list under the catalog.
    Device(Vec<crate::screens::DeviceApp>),
    /// A build was downloaded and handed to the system's installer: it is on the ledger now.
    Installed {
        program: String,
        version_code: i64,
        version_name: String,
        path: String,
    },
    /// The session ended (the refresh token was refused): show the sign-in again.
    SignedOut,
}

/// The app and the function that feeds it.
pub struct Hoist {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Hoist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hoist").finish_non_exhaustive()
    }
}

/// What the host knows that the app needs.
#[derive(Debug, Clone)]
pub struct Setup {
    pub server: String,
    /// Whether a refresh token is stored: the app starts by asking for the catalog instead of for a password.
    pub has_session: bool,
    pub installed: BTreeMap<String, Installed>,
    /// The program that is this app (listed last when updating everything).
    pub self_program: String,
    /// How often to look for updates in the background while signed in (milliseconds; 0 for never). The "Check
    /// for updates automatically" setting switches it off and on.
    pub check_every_ms: u32,
    /// Whether that check is switched on.
    pub auto_check: bool,
    /// Whether there is a public catalogue tab (Android installs from it; the desktop has none).
    pub public: bool,
}

/// Where the messages go once the views exist.
type Inbox = Rc<std::cell::RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn hoist(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Hoist {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || {
        let signed_in = create_signal(setup.has_session);
        let entries = create_signal(Vec::<Entry>::new());
        let message = create_signal(String::new());
        let busy = create_signal(false);
        let screen = create_signal(0_usize);
        // The program whose page is open, if one is.
        let selected = create_signal(None::<String>);
        let prefs = Settings::new(&setup.server, setup.auto_check);
        // Whether the list of what has an update is open, and what the search field holds.
        let updates_open = create_signal(false);
        let search = create_signal(String::new());
        // The other apps of the device, listed under the registry's.
        let device = create_signal(Vec::<crate::screens::DeviceApp>::new());
        // Whether the catalog being waited for was asked for by the timer, what it found last time, and whether
        // the timer is running.
        let background = Rc::new(std::cell::Cell::new(false));
        // Whether the catalog being waited for was asked for by the user (the refresh button, "Check now").
        let asked = Rc::new(std::cell::Cell::new(false));
        let known_updates = Rc::new(std::cell::Cell::new(None::<usize>));
        let checking = Rc::new(std::cell::Cell::new(false));
        let raw = create_signal(Vec::<Artifact>::new());
        let installed = Rc::new(std::cell::RefCell::new(setup.installed.clone()));
        let self_program = setup.self_program.clone();
        // The public catalogue: what it installs goes through the same busy state and installer as the registry's.
        let public = {
            let (install_requests, open_requests) = (Rc::clone(&requests), Rc::clone(&requests));
            let (busy, message, device, ledger) = (
                busy.clone(),
                message.clone(),
                device.clone(),
                Rc::clone(&installed),
            );
            crate::public::PublicState::new(
                Rc::clone(&requests),
                Rc::new(move |app| {
                    message.set(String::new());
                    busy.set(true);
                    install_requests(Request::InstallPublic(app));
                }),
                Rc::new(move |program| open_requests(Request::Launch(program))),
                Rc::new(move |program: &str| {
                    device.get().iter().any(|d| d.program == program)
                        || ledger.borrow().contains_key(program)
                }),
            )
        };
        let public_tab = setup.public;
        {
            let (signed_in, entries, message, busy, raw) = (
                signed_in.clone(),
                entries.clone(),
                message.clone(),
                busy.clone(),
                raw.clone(),
            );
            let (requests, installed, self_program) = (
                Rc::clone(&requests),
                Rc::clone(&installed),
                self_program.clone(),
            );
            let (background, asked, known_updates, checking) = (
                Rc::clone(&background),
                Rc::clone(&asked),
                Rc::clone(&known_updates),
                Rc::clone(&checking),
            );
            let (auto_check, every) = (prefs.auto_check.clone(), setup.check_every_ms);
            let (device_in, public_in) = (device.clone(), public.clone());
            *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| match m {
                Msg::LoggedIn => {
                    signed_in.set(true);
                    message.set("Signed in".into());
                    requests(Request::Catalog);
                }
                Msg::LoginFailed(why) | Msg::CatalogFailed(why) | Msg::DownloadFailed(why) => {
                    message.set(why);
                    busy.set(false);
                }
                Msg::Catalog(rows) => {
                    let built = build_entries(&rows, &installed.borrow(), &self_program);
                    let updates = built
                        .iter()
                        .filter(|e| e.state() == hoist_core::InstallState::UpdateAvailable)
                        .count();
                    // A look in the background says so when it found something new; one the user asked for shows.
                    let found = background.replace(false)
                        && known_updates.get().is_some_and(|before| updates > before);
                    known_updates.set(Some(updates));
                    entries.set(built);
                    raw.set(rows);
                    message.set(if found {
                        format!(
                            "{updates} update{} available",
                            if updates == 1 { "" } else { "s" }
                        )
                    } else if asked.replace(false) {
                        let said = match updates {
                            0 => String::new(),
                            1 => "1 update available".to_string(),
                            n => format!("{n} updates available"),
                        };
                        // It stays long enough to read, then goes.
                        let (note, said_again) = (message.clone(), said.clone());
                        quench_ui::platform::after(4000, move || {
                            if note.get_untracked() == said_again {
                                note.set(String::new());
                            }
                        });
                        said
                    } else {
                        String::new()
                    });
                    busy.set(false);
                    if every > 0 && !checking.replace(true) {
                        schedule_check(
                            every,
                            Rc::clone(&requests),
                            auto_check.clone(),
                            signed_in.clone(),
                            busy.clone(),
                            Rc::clone(&background),
                        );
                    }
                }
                Msg::Downloaded { program, path } => {
                    message.set(format!("Downloaded {program} to {path}"));
                    busy.set(false);
                }
                Msg::Installed {
                    program,
                    version_code,
                    version_name,
                    path,
                } => {
                    message.set(format!(
                        "Opened {program} {version_name} in your installer ({path})"
                    ));
                    installed.borrow_mut().insert(
                        program.clone(),
                        Installed {
                            program,
                            version_code,
                            version_name,
                        },
                    );
                    entries.set(build_entries(
                        &raw.get(),
                        &installed.borrow(),
                        &self_program,
                    ));
                    busy.set(false);
                }
                Msg::Device(list) => device_in.set(list),
                Msg::Categories(rows) => public_in.on_categories(rows),
                Msg::PublicPage { asked, page } => public_in.on_page(&asked, page),
                Msg::PublicFailed { why, .. } => public_in.on_failed(&why),
                Msg::Known(list) => {
                    installed
                        .borrow_mut()
                        .extend(list.into_iter().map(|i| (i.program.clone(), i)));
                    entries.set(build_entries(
                        &raw.get(),
                        &installed.borrow(),
                        &self_program,
                    ));
                }
                Msg::SignedOut => {
                    signed_in.set(false);
                    message.set("Sign in again".into());
                    busy.set(false);
                }
            }));
        }
        if setup.has_session {
            busy.set(true);
            requests(Request::Catalog);
        }
        let act: Act = {
            let (requests, raw, message, busy) = (
                Rc::clone(&requests),
                raw.clone(),
                message.clone(),
                busy.clone(),
            );
            let (installed, self_program) = (Rc::clone(&installed), self_program.clone());
            let selected = selected.clone();
            Rc::new(move |action: Action| {
                let wanted = match &action {
                    Action::Details(p) => {
                        selected.set(Some(p.clone()));
                        None
                    }
                    Action::Install(p) | Action::Update(p) => Some(p.clone()),
                    Action::Open(p) => {
                        requests(Request::Launch(p.clone()));
                        None
                    }
                    Action::UpdateAll => {
                        // Everything that has an update, this app last: updating it may restart it.
                        let mut due: Vec<Entry> =
                            build_entries(&raw.get(), &installed.borrow(), &self_program)
                                .into_iter()
                                .filter(|e| e.state() == hoist_core::InstallState::UpdateAvailable)
                                .collect();
                        due.sort_by_key(|e| e.is_self);
                        if !due.is_empty() {
                            busy.set(true);
                            message.set(format!("Updating {} programs...", due.len()));
                            for entry in due {
                                requests(Request::Install(entry.latest));
                            }
                        }
                        None
                    }
                };
                if let Some(program) = wanted
                    && let Some(artifact) = raw
                        .get()
                        .into_iter()
                        .filter(|a| a.program == program)
                        .max_by_key(|a| a.version_code)
                {
                    busy.set(true);
                    message.set(format!("Downloading {}...", artifact.title()));
                    requests(Request::Install(artifact));
                }
            })
        };
        let (login_requests, login_busy, server) =
            (Rc::clone(&requests), busy.clone(), setup.server.clone());
        let build_login =
            move || login_form(&server, Rc::clone(&login_requests), login_busy.clone());
        let (signed, panels_entries, panels_screen) =
            (signed_in.clone(), entries.clone(), screen.clone());
        let panels_device = device.clone();
        let (panels_selected, panels_updates, panels_search) =
            (selected.clone(), updates_open.clone(), search.clone());
        // Settings are kept as they change: a valid server address (a moment after the last key) and the switch.
        {
            let (server, auto, requests) = (
                prefs.registry_url.clone(),
                prefs.auto_check.clone(),
                Rc::clone(&requests),
            );
            let (first, timer) = (
                Rc::new(std::cell::Cell::new(true)),
                Rc::new(std::cell::Cell::new(None::<quench_ui::platform::Timer>)),
            );
            create_effect(move || {
                let (address, on) = (server.value(), auto.get());
                if first.replace(false) {
                    return;
                }
                if let Some(t) = timer.take() {
                    t.cancel();
                }
                if !(address.starts_with("https://") || address.starts_with("http://")) {
                    return;
                }
                let requests = Rc::clone(&requests);
                timer.set(Some(quench_ui::platform::after(600, move || {
                    requests(Request::Preferences {
                        server: address,
                        auto_check: on,
                    });
                })));
            });
        }
        // Looking now, because the user asked: says so while it waits and what it found.
        let look: Rc<dyn Fn()> = {
            let (requests, asked, message, busy) = (
                Rc::clone(&requests),
                Rc::clone(&asked),
                message.clone(),
                busy.clone(),
            );
            Rc::new(move || {
                asked.set(true);
                message.set(String::new());
                busy.set(true);
                requests(Request::Catalog);
            })
        };
        let panel_look = Rc::clone(&look);
        let panels_public = public.clone();
        // Signing out asks first.
        let confirm_out = create_signal(false);
        let panel_confirm = confirm_out.clone();
        let panels = View::dynamic(move || {
            if !signed.get() {
                return build_login();
            }
            if panels_screen.get() == 0 {
                // A program's page, when one is open and still in the catalog.
                if let Some(program) = panels_selected.get()
                    && let Some(entry) = panels_entries
                        .get()
                        .into_iter()
                        .find(|e| e.latest.program == program)
                {
                    return detail(&entry, &act);
                }
                if panels_updates.get() {
                    return updates(&panels_entries, &act);
                }
                catalog(&panels_entries, &panels_device, &act, &panels_search)
            } else if public_tab && panels_screen.get() == 1 {
                crate::public::public_screen(&panels_public)
            } else {
                let (now, out) = (Rc::clone(&panel_look), panel_confirm.clone());
                settings(
                    &prefs,
                    concat!("v", env!("CARGO_PKG_VERSION")),
                    move || now(),
                    move || out.set(true),
                )
            }
        });
        // The bar says where you are: the screen's name, an arrow back from a program's page or the updates, and on
        // the catalog the ways to the updates, to a refresh and out.
        let bar = {
            let (signed, screen, selected, updates_open, entries) = (
                signed_in.clone(),
                screen.clone(),
                selected.clone(),
                updates_open.clone(),
                entries.clone(),
            );
            let bar_confirm = confirm_out.clone();
            let bar_public = public.clone();
            let bar_look = Rc::clone(&look);
            View::dynamic(move || {
                if !signed.get() {
                    return app_bar("Set up Hoist", None, Vec::new()).into_view();
                }
                if public_tab && screen.get() == 1 {
                    let leading =
                        (bar_public.view.get() != crate::public::PublicView::Home).then(|| {
                            let state = bar_public.clone();
                            icon_button("Back", "", move || state.back())
                                .child(crate::icons::back("q-icon"))
                                .attr("id", "public-back")
                                .into_view()
                        });
                    return app_bar(&bar_public.title(), leading, Vec::new()).into_view();
                }
                if screen.get() != 0 {
                    return app_bar("Settings", None, Vec::new()).into_view();
                }
                let back = |close: Box<dyn Fn()>| {
                    icon_button("Back", "", close)
                        .child(crate::icons::back("q-icon"))
                        .attr("id", "detail-back")
                        .into_view()
                };
                if let Some(program) = selected.get() {
                    let title = entries
                        .get()
                        .into_iter()
                        .find(|e| e.latest.program == program)
                        .map_or_else(|| program.clone(), |e| e.latest.title().to_string());
                    let close = selected.clone();
                    return app_bar(
                        &title,
                        Some(back(Box::new(move || close.set(None)))),
                        Vec::new(),
                    )
                    .into_view();
                }
                if updates_open.get() {
                    let close = updates_open.clone();
                    return app_bar(
                        "Updates",
                        Some(back(Box::new(move || close.set(false)))),
                        Vec::new(),
                    )
                    .into_view();
                }
                let (open, refresh, out) = (
                    updates_open.clone(),
                    Rc::clone(&bar_look),
                    bar_confirm.clone(),
                );
                app_bar(
                    "Catalog",
                    None,
                    vec![
                        icon_button("Updates", "", move || open.set(true))
                            .child(crate::icons::download("q-icon"))
                            .attr("id", "updates-open")
                            .into_view(),
                        icon_button("Refresh", "", move || refresh())
                            .child(crate::icons::refresh("q-icon"))
                            .attr("id", "refresh")
                            .into_view(),
                        icon_button("Sign out", "", move || out.set(true))
                            .child(crate::icons::sign_out("q-icon"))
                            .attr("id", "logout-bar")
                            .into_view(),
                    ],
                )
                .into_view()
            })
        };
        let (shown, working) = (message.clone(), busy.clone());
        div()
            .attr("id", "hoist")
            .child(bar)
            .child(div().class("hoist-body").child(panels))
            .child(
                p().attr("id", "message")
                    .attr("role", "status")
                    .text_fn(move || shown.get()),
            )
            .child(View::show(
                move || working.get(),
                || {
                    // Dots that take turns to light: something is going on, and the screen is not to be tapped.
                    div()
                        .attr("id", "loader")
                        .attr("role", "progressbar")
                        .attr("aria-label", "Working")
                        .class("hoist-loader")
                        .child(span().class("hoist-dot"))
                        .child(span().class("hoist-dot hoist-dot-2"))
                        .child(span().class("hoist-dot hoist-dot-3"))
                        .into_view()
                },
            ))
            .child({
                let (leave, stay, go) = (
                    Rc::clone(&requests),
                    confirm_out.clone(),
                    confirm_out.clone(),
                );
                dialog(
                    &confirm_out,
                    "Sign out?",
                    || {
                        p().class("hoist-note")
                            .text("You will have to sign in again on this device.")
                            .into_view()
                    },
                    move || {
                        let (stay, go, leave) = (stay.clone(), go.clone(), Rc::clone(&leave));
                        vec![
                            button()
                                .attr("type", "button")
                                .attr("id", "cancel-logout")
                                .text("Cancel")
                                .on_click(move || stay.set(false))
                                .into_view(),
                            button()
                                .attr("type", "button")
                                .attr("id", "confirm-logout")
                                .class("hoist-danger")
                                .text("Sign out")
                                .on_click(move || {
                                    go.set(false);
                                    leave(Request::Logout);
                                })
                                .into_view(),
                        ]
                    },
                )
            })
            .child(View::show(
                {
                    let signed = signed_in.clone();
                    move || signed.get()
                },
                move || {
                    let mut items: Vec<crate::screens::Tab> =
                        vec![("Catalog", crate::icons::catalog)];
                    if public_tab {
                        items.push(("Public", crate::icons::public));
                    }
                    items.push(("Settings", crate::icons::gear));
                    crate::screens::bottom_bar(items, &screen).into_view()
                },
            ))
            .into_view()
    });
    Hoist { view, deliver }
}

/// Looks for updates now and then: every `every` milliseconds, if the setting is on, someone is signed in and
/// nothing else is being waited for, it asks for the catalog (and marks that the timer did, so a finding is announced).
fn schedule_check(
    every: u32,
    requests: Rc<dyn Fn(Request)>,
    enabled: Signal<bool>,
    signed_in: Signal<bool>,
    busy: Signal<bool>,
    background: Rc<std::cell::Cell<bool>>,
) {
    // Without a platform a timer would run at once, and this would never stop.
    if !quench_ui::platform::installed() {
        return;
    }
    quench_ui::platform::after(every, move || {
        if enabled.get() && signed_in.get() && !busy.get() {
            background.set(true);
            requests(Request::Catalog);
        }
        schedule_check(every, requests, enabled, signed_in, busy, background);
    });
}

/// The sign-in form: the server, a user name and a password; the password goes to the host once and is not kept.
fn login_form(server: &str, requests: Rc<dyn Fn(Request)>, busy: Signal<bool>) -> View {
    let server_field = Field::new("server", server).rule(rules::check(
        |v| v.starts_with("https://") || v.starts_with("http://"),
        "The address starts with https://",
    ));
    let user = Field::new("username", "").rule(rules::required("Enter your user name"));
    let password = Field::new("password", "").rule(rules::required("Enter your password"));
    let form = Form::new()
        .field(&server_field)
        .field(&user)
        .field(&password);
    let (s, u, p) = (server_field.clone(), user.clone(), password.clone());
    div()
        .attr("id", "login")
        .class("hoist-screen hoist-scrolls")
        .child(h2().text("Point Hoist at your server"))
        .child(
            form.element(move |_| {
                busy.set(true);
                requests(Request::Login {
                    server: s.value(),
                    username: u.value(),
                    password: p.value(),
                });
            })
            .attr("id", "login-form")
            .child(text_field("Server URL", &server_field, "url", None))
            .child(text_field("Username", &user, "text", None))
            .child(text_field("Password", &password, "password", None))
            .child(form.error_view())
            .child(
                button()
                    .attr("type", "submit")
                    .attr("id", "sign-in")
                    .text("Log in"),
            ),
        )
        .into_view()
}
