//! The app against a host: sign in, watch the backup, change the settings. The app does no I/O itself: it asks for
//! things through a `requests` function and is told what happened through `deliver` (both carry plain data), so the
//! host decides how to do the work and where the answers come in.

use std::rc::Rc;

use forklift_core::{Folder, Settings, Snapshot, clean_server};
use quench_ui::form::{Field, Form, rules};
use quench_ui::kit::{app_bar, dialog, text_field};
use quench_ui::prelude::*;

use crate::screens::{
    About, Actions, Chooser, Controls, bottom_bar, dashboard, restore_words, settings,
    storage_screen,
};

/// What the app asks its host to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Login {
        server: String,
        username: String,
        password: String,
    },
    /// Ask the server which storages this account may use.
    ListStorages,
    /// The person chose where this device's backup goes.
    ChooseStorage(String),
    /// Look for files now and send what is waiting; the person asked, so failed files are tried again.
    BackUpNow,
    /// A setting changed: the host keeps it and acts on it (a folder switched on is scanned).
    Settings(Settings),
    /// Let the person pick a folder.
    AddFolder,
    /// Read what is stored and work out what this device lacks.
    PlanRestore,
    /// The person said yes to the plan.
    StartRestore,
    /// The person stopped a restore.
    CancelRestore,
    Logout,
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    LoggedIn,
    LoginFailed(String),
    Snapshot(Box<Snapshot>),
    /// The settings changed on the host's side (a folder was added).
    Settings(Box<Settings>),
    /// The storages the account may use (or why they could not be read).
    Storages(Box<Result<Vec<forklift_core::StorageInfo>, String>>),
    /// Where a restore is.
    Restore(Box<forklift_core::RestoreView>),
    /// Something to tell the person.
    Notice(String),
    /// The session ended: show the sign-in again.
    SignedOut,
}

/// The app and the function that feeds it.
pub struct Forklift {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Forklift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Forklift").finish_non_exhaustive()
    }
}

/// What the host knows that the app needs.
#[derive(Debug, Clone)]
pub struct Setup {
    pub settings: Settings,
    /// Whether a refresh token is stored: the app starts on the dashboard instead of asking for a password.
    pub has_session: bool,
    /// Whether this is a phone (a media library, a battery) or a computer (folders only).
    pub phone: bool,
    /// The zone's offset from UTC in minutes, for the time of the last check.
    pub offset_minutes: i32,
    /// How long a message stays, in milliseconds; 0 keeps it until the next one.
    pub toast_ms: u32,
    /// What the dashboard showed when the app was last used: on the screen at once, until the host has looked again.
    pub initial: Snapshot,
    /// How long the loader stays once it has come up (see `toast::lingering`).
    pub loader_min_ms: u32,
}

type Inbox = Rc<std::cell::RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn forklift(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Forklift {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || {
        let signed_in = create_signal(setup.has_session);
        let shot = create_signal(setup.initial.clone());
        let message = create_signal(String::new());
        let busy = create_signal(false);
        let screen = create_signal(0_usize);
        let confirm_out = create_signal(false);
        let restore = create_signal(forklift_core::RestoreView::Idle);
        let restore_open = create_signal(false);
        // The storage chooser: before anything is sent when none was chosen yet, and when the person wants another.
        let needs_storage = setup.has_session && !setup.settings.storage_confirmed;
        let chooser = create_signal(if needs_storage {
            Chooser::Loading
        } else {
            Chooser::Hidden
        });
        if needs_storage {
            requests(Request::ListStorages);
        }
        let s = &setup.settings;
        let controls = Controls {
            auto: create_signal(s.auto),
            photos: create_signal(s.photos),
            videos: create_signal(s.videos),
            videos_wifi_only: create_signal(s.videos_wifi_only),
            charging_for_large: create_signal(s.charging_for_large),
            folders: create_signal(s.folders.clone()),
        };
        let kept = Rc::new(std::cell::RefCell::new(setup.settings.clone()));

        // What the host says comes to the signals.
        {
            let (signed_in, shot, message, busy, controls, kept, restore, chooser, ask) = (
                signed_in.clone(),
                shot.clone(),
                message.clone(),
                busy.clone(),
                controls.clone(),
                Rc::clone(&kept),
                restore.clone(),
                chooser.clone(),
                Rc::clone(&requests),
            );
            *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| match m {
                Msg::LoggedIn => {
                    busy.set(false);
                    signed_in.set(true);
                    // Nothing is sent before the person has said where it goes.
                    if !kept.borrow().storage_confirmed {
                        chooser.set(Chooser::Loading);
                        ask(Request::ListStorages);
                    }
                }
                Msg::Storages(result) => {
                    if chooser.get() != Chooser::Hidden {
                        chooser.set(match *result {
                            Ok(list) => Chooser::Choose(list),
                            Err(why) => Chooser::Failed(why),
                        });
                    }
                }
                Msg::LoginFailed(why) => {
                    busy.set(false);
                    message.set(why);
                }
                Msg::Snapshot(s) => shot.set(*s),
                Msg::Restore(v) => restore.set(*v),
                Msg::Settings(s) => {
                    controls.folders.set(s.folders.clone());
                    *kept.borrow_mut() = *s;
                }
                Msg::Notice(text) => {
                    busy.set(false);
                    message.set(text);
                }
                Msg::SignedOut => {
                    busy.set(false);
                    signed_in.set(false);
                    message.set("Sign in again".into());
                }
            }));
        }

        // A change to any setting goes to the host, once it has settled (a typed name is not sent letter by letter).
        {
            let (requests, controls, kept) =
                (Rc::clone(&requests), controls.clone(), Rc::clone(&kept));
            let (first, timer) = (
                Rc::new(std::cell::Cell::new(true)),
                Rc::new(std::cell::Cell::new(None::<quench_ui::platform::Timer>)),
            );
            create_effect(move || {
                let next = Settings {
                    server: kept.borrow().server.clone(),
                    storage: kept.borrow().storage.clone(),
                    storage_confirmed: kept.borrow().storage_confirmed,
                    auto: controls.auto.get(),
                    photos: controls.photos.get(),
                    videos: controls.videos.get(),
                    videos_wifi_only: controls.videos_wifi_only.get(),
                    charging_for_large: controls.charging_for_large.get(),
                    concurrency: kept.borrow().concurrency,
                    folders: controls.folders.get(),
                };
                if first.replace(false) {
                    return;
                }
                if let Some(t) = timer.take() {
                    t.cancel();
                }
                if next == *kept.borrow() {
                    return;
                }
                let (requests, kept) = (Rc::clone(&requests), Rc::clone(&kept));
                timer.set(Some(quench_ui::platform::after(600, move || {
                    *kept.borrow_mut() = next.clone();
                    requests(Request::Settings(next));
                })));
            });
        }

        let back_up: Rc<dyn Fn()> = {
            let (requests, message) = (Rc::clone(&requests), message.clone());
            Rc::new(move || {
                message.set(String::new());
                requests(Request::BackUpNow);
            })
        };
        let panels = {
            let (signed, screen, shot, controls, requests, confirm, kept, chooser) = (
                signed_in.clone(),
                screen.clone(),
                shot.clone(),
                controls.clone(),
                Rc::clone(&requests),
                confirm_out.clone(),
                Rc::clone(&kept),
                chooser.clone(),
            );
            let (offset, busy_login, phone) = (setup.offset_minutes, busy.clone(), setup.phone);
            View::dynamic(move || {
                if !signed.get() {
                    return login_form(
                        &kept.borrow().server,
                        Rc::clone(&requests),
                        busy_login.clone(),
                    );
                }
                if chooser.get() != Chooser::Hidden {
                    let (pick, retry, close) =
                        (Rc::clone(&requests), Rc::clone(&requests), chooser.clone());
                    let picked = chooser.clone();
                    return storage_screen(
                        &chooser,
                        &kept.borrow().storage,
                        kept.borrow().storage_confirmed,
                        Rc::new(move |name| {
                            picked.set(Chooser::Hidden);
                            pick(Request::ChooseStorage(name));
                        }),
                        Rc::new(move || retry(Request::ListStorages)),
                        Rc::new(move || close.set(Chooser::Hidden)),
                    );
                }
                if screen.get() == 0 {
                    return dashboard(&shot, offset, Rc::clone(&back_up));
                }
                let (add, remove, out, ask_restore, ask_storages, open_chooser) = (
                    Rc::clone(&requests),
                    controls.folders.clone(),
                    confirm.clone(),
                    Rc::clone(&requests),
                    Rc::clone(&requests),
                    chooser.clone(),
                );
                let drop_folder: Rc<dyn Fn(String)> = Rc::new(move |slug| {
                    remove.set(
                        remove
                            .get()
                            .into_iter()
                            .filter(|f: &Folder| f.slug != slug)
                            .collect(),
                    );
                });
                let kept = kept.borrow();
                settings(
                    &About {
                        phone,
                        server: &kept.server,
                        storage: &kept.storage,
                        version: concat!("v", env!("CARGO_PKG_VERSION")),
                    },
                    &controls,
                    Actions {
                        add_folder: Rc::new(move || add(Request::AddFolder)),
                        remove_folder: drop_folder,
                        change_storage: Rc::new(move || {
                            open_chooser.set(Chooser::Loading);
                            ask_storages(Request::ListStorages);
                        }),
                        restore: Rc::new(move || ask_restore(Request::PlanRestore)),
                        sign_out: Rc::new(move || out.set(true)),
                    },
                )
            })
        };
        let bar = {
            let (signed, screen, chooser) = (signed_in.clone(), screen.clone(), chooser.clone());
            View::dynamic(move || {
                if !signed.get() {
                    return app_bar("Set up Forklift", None, Vec::new()).into_view();
                }
                if chooser.get() != Chooser::Hidden {
                    return app_bar("Storage", None, Vec::new()).into_view();
                }
                app_bar(
                    if screen.get() == 0 {
                        "Backup"
                    } else {
                        "Settings"
                    },
                    None,
                    Vec::new(),
                )
                .into_view()
            })
        };
        {
            let (restore, open) = (restore.clone(), restore_open.clone());
            create_effect(move || open.set(restore.get() != forklift_core::RestoreView::Idle));
        }
        if setup.toast_ms > 0 {
            crate::toast::dismiss_after(&message, setup.toast_ms);
        }
        let shown = message.clone();
        let working = crate::toast::lingering(&busy, setup.loader_min_ms);
        div()
            .attr("id", "forklift")
            .child(bar)
            .child(div().class("forklift-body").child(panels))
            .child(crate::toast::toast(&shown))
            .child(View::show(
                move || working.get(),
                || {
                    div()
                        .attr("id", "loader")
                        .attr("role", "progressbar")
                        .attr("aria-label", "Working")
                        .class("forklift-loader")
                        .child(span().class("forklift-dot"))
                        .child(span().class("forklift-dot forklift-dot-2"))
                        .child(span().class("forklift-dot forklift-dot-3"))
                        .into_view()
                },
            ))
            .child(restore_dialog(&restore, &restore_open, Rc::clone(&requests)))
            .child({
                let (leave, stay, go) = (Rc::clone(&requests), confirm_out.clone(), confirm_out.clone());
                dialog(
                    &confirm_out,
                    "Sign out?",
                    || {
                        p().class("forklift-note")
                            .text("Backing up stops until you sign in again on this device. What is already stored stays.")
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
                                .class("forklift-danger")
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
                    let (signed, chooser) = (signed_in.clone(), chooser.clone());
                    // No tabs while the storage is being chosen: there is nowhere else to go yet.
                    move || signed.get() && chooser.get() == Chooser::Hidden
                },
                move || {
                    bottom_bar(vec![("Backup", crate::icons::backup), ("Settings", crate::icons::gear)], &screen).into_view()
                },
            ))
            .into_view()
    });
    Forklift { view, deliver }
}

/// A password field with an eye beside it: what is typed stays hidden until the eye is pressed.
fn password_field(text: &str, field: &Field) -> Element {
    let shown = create_signal(false);
    let (kind, flip, icon_when) = (shown.clone(), shown.clone(), shown);
    let control = field
        .input()
        .class("q-textfield-input forklift-password-input")
        .attr_fn("type", move || {
            if kind.get() { "text" } else { "password" }.to_string()
        });
    let eye = button()
        .attr("type", "button")
        .attr("id", "show-password")
        .attr("aria-label", "Show password")
        .class("q-iconbtn forklift-eye")
        .child(View::dynamic(move || {
            if icon_when.get() {
                crate::icons::eye_off("q-icon").into_view()
            } else {
                crate::icons::eye("q-icon").into_view()
            }
        }))
        .on_click(move || flip.set(!flip.get()));
    div()
        .class("q-textfield")
        .child(
            label()
                .attr("for", field.name())
                .class("q-textfield-label")
                .text(text),
        )
        .child(div().class("forklift-password").child(control).child(eye))
        .child(field.error_view().class("q-textfield-error"))
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
    let (srv, usr, pwd) = (server_field.clone(), user.clone(), password.clone());
    div()
        .attr("id", "login")
        .class("forklift-screen forklift-scrolls")
        .child(h2().text("Point Forklift at your server"))
        .child(
            form.element(move |_| {
                busy.set(true);
                requests(Request::Login {
                    server: clean_server(&srv.value()),
                    // A stray space (a keyboard adds one after a suggestion) is not part of a user name.
                    username: usr.value().trim().to_string(),
                    password: pwd.value(),
                });
            })
            .attr("id", "login-form")
            .child(text_field("Server URL", &server_field, "url", None))
            .child(text_field("Username", &user, "text", None))
            .child(password_field("Password", &password))
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

/// The dialog of a restore: what is about to happen and a way to say yes or no, then how it goes, then how it went.
fn restore_dialog(
    view: &Signal<forklift_core::RestoreView>,
    open: &Signal<bool>,
    requests: Rc<dyn Fn(Request)>,
) -> View {
    use forklift_core::RestoreView as R;
    let words = view.clone();
    let actions = view.clone();
    let close = view.clone();
    dialog(
        open,
        "Restore from backup",
        move || {
            let words = words.clone();
            p().attr("id", "restore-words")
                .class("forklift-note")
                .text_fn(move || restore_words(&words.get()))
                .into_view()
        },
        move || {
            let (view, close, requests) = (actions.clone(), close.clone(), Rc::clone(&requests));
            vec![View::dynamic(move || {
                let (go, stop, ok, no) = (
                    Rc::clone(&requests),
                    Rc::clone(&requests),
                    close.clone(),
                    close.clone(),
                );
                let button_row = |buttons: Vec<View>| {
                    div()
                        .class("forklift-actions")
                        .children(buttons)
                        .into_view()
                };
                match view.get() {
                    R::Plan(s) if s.files > 0 => button_row(vec![
                        button()
                            .attr("type", "button")
                            .attr("id", "restore-cancel")
                            .text("Cancel")
                            .on_click(move || no.set(R::Idle))
                            .into_view(),
                        button()
                            .attr("type", "button")
                            .attr("id", "restore-go")
                            .class("forklift-primary-small")
                            .text("Restore")
                            .on_click(move || go(Request::StartRestore))
                            .into_view(),
                    ]),
                    R::Running(_) | R::Planning => button_row(vec![
                        button()
                            .attr("type", "button")
                            .attr("id", "restore-stop")
                            .text("Stop")
                            .on_click(move || stop(Request::CancelRestore))
                            .into_view(),
                    ]),
                    _ => button_row(vec![
                        button()
                            .attr("type", "button")
                            .attr("id", "restore-ok")
                            .text("OK")
                            .on_click(move || ok.set(R::Idle))
                            .into_view(),
                    ]),
                }
            })]
        },
    )
}
