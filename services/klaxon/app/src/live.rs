//! The app against a host: the list of alarms and the editor.
//!
//! The app does no I/O itself: it asks for things through a `requests` function and is told what happened through
//! `deliver` (both carry plain data), so the host decides how to do the work and where the answers come in.

use std::cell::RefCell;
use std::rc::Rc;

use klaxon_core::{Alarm, Alarms, Now};
use quench_ui::kit::app_bar;
use quench_ui::prelude::*;

use crate::icons;
use crate::screens::{Draft, editor, list};

/// What the app asks its host to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Keep this alarm (new, or a changed one) and set the clock for the next ring.
    Save(Alarm),
    Delete(u32),
    Toggle(u32, bool),
    /// Ask the system for what an alarm needs to wake the screen (notifications).
    AllowAlerts,
    /// Let the person choose an alarm sound (the system's picker); the answer is [`Msg::Sound`].
    PickSound(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    /// The alarms as the host has them (a one-time alarm switched itself off).
    Alarms(Vec<Alarm>),
    /// The local time, again.
    Clock(Now),
    /// Whether the system lets an alarm show over the lock screen.
    Alerts(bool),
    /// The sound the person chose: its URI (empty: the default) and its name.
    Sound { uri: String, name: String },
    /// Something to tell the person.
    Notice(String),
}

/// What the host knows that the app needs.
#[derive(Debug, Clone)]
pub struct Setup {
    pub alarms: Vec<Alarm>,
    pub now: Now,
    pub alerts_ok: bool,
    /// How long a message stays, in milliseconds; 0 keeps it until the next one.
    pub toast_ms: u32,
}

/// The app and the functions that feed it.
pub struct Klaxon {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
    /// The phone's Back button: closes the editor. False when there is nothing to close, and the host leaves.
    pub back: Rc<dyn Fn() -> bool>,
}

impl std::fmt::Debug for Klaxon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Klaxon").finish_non_exhaustive()
    }
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;
type Editing = Rc<RefCell<Option<Signal<Option<Draft>>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one function wires the whole app: its signals and every screen's callbacks share them"
)]
pub fn klaxon(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Klaxon {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    // The editor's signal, for Back (made when the view is).
    let editing_for_back: Editing = Rc::default();
    let back_to = Rc::clone(&editing_for_back);
    let back: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let Some(editing) = back_to.borrow().clone() else {
            return false;
        };
        if editing.get_untracked().is_some() {
            editing.set(None);
            true
        } else {
            false
        }
    });
    let view = View::lazy(move || {
        let alarms = create_signal(setup.alarms.clone());
        let now = create_signal(setup.now);
        let alerts_ok = create_signal(setup.alerts_ok);
        let message = create_signal(String::new());
        let editing: Signal<Option<Draft>> = create_signal(None);
        *editing_for_back.borrow_mut() = Some(editing.clone());

        {
            let (alarms, now, alerts_ok, message, editing) = (
                alarms.clone(),
                now.clone(),
                alerts_ok.clone(),
                message.clone(),
                editing.clone(),
            );
            *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| match m {
                Msg::Alarms(list) => {
                    if alarms.get_untracked() != list {
                        alarms.set(list);
                    }
                }
                Msg::Clock(at) => {
                    if now.get_untracked() != at {
                        now.set(at);
                    }
                }
                Msg::Alerts(ok) => alerts_ok.set(ok),
                Msg::Sound { uri, name } => {
                    if let Some(draft) = editing.get_untracked() {
                        draft.sound.set(uri);
                        draft.sound_name.set(name);
                    }
                }
                Msg::Notice(text) => message.set(text),
            }));
        }
        if setup.toast_ms > 0 {
            dismiss_after(&message, setup.toast_ms);
        }

        // The alarms change here at once, and the host is told.
        let change = {
            let alarms = alarms.clone();
            Rc::new(move |f: &dyn Fn(&mut Alarms)| {
                let mut all = Alarms {
                    list: alarms.get_untracked(),
                };
                f(&mut all);
                alarms.set(all.list);
            })
        };
        let toggle: Rc<dyn Fn(u32, bool)> = {
            let (requests, change) = (Rc::clone(&requests), Rc::clone(&change));
            Rc::new(move |id, on| {
                change(&|a| a.set_enabled(id, on));
                requests(Request::Toggle(id, on));
            })
        };
        let open: Rc<dyn Fn(u32)> = {
            let (alarms, editing) = (alarms.clone(), editing.clone());
            Rc::new(move |id| {
                if let Some(a) = alarms.get_untracked().iter().find(|a| a.id == id) {
                    editing.set(Some(Draft::new(a, false)));
                }
            })
        };
        let save: Rc<dyn Fn(Alarm)> = {
            let (requests, change, editing, message) = (
                Rc::clone(&requests),
                Rc::clone(&change),
                editing.clone(),
                message.clone(),
            );
            Rc::new(move |alarm| {
                change(&|a| a.put(alarm.clone()));
                editing.set(None);
                message.set(format!(
                    "Alarm set for {}",
                    klaxon_core::clock(alarm.hour, alarm.minute)
                ));
                requests(Request::Save(alarm));
            })
        };
        let delete: Rc<dyn Fn(u32)> = {
            let (requests, change, editing, message) = (
                Rc::clone(&requests),
                Rc::clone(&change),
                editing.clone(),
                message.clone(),
            );
            Rc::new(move |id| {
                change(&|a| a.remove(id));
                editing.set(None);
                message.set("Alarm deleted".into());
                requests(Request::Delete(id));
            })
        };
        let cancel: Rc<dyn Fn()> = {
            let editing = editing.clone();
            Rc::new(move || editing.set(None))
        };

        let bar = {
            let (editing, cancel) = (editing.clone(), Rc::clone(&cancel));
            View::dynamic(move || match editing.get() {
                Some(d) => {
                    let cancel = Rc::clone(&cancel);
                    app_bar(
                        if d.fresh { "New alarm" } else { "Edit alarm" },
                        Some(
                            button()
                                .attr("type", "button")
                                .attr("id", "back")
                                .attr("aria-label", "Back")
                                .class("q-iconbtn")
                                .child(icons::back("q-icon"))
                                .on_click(move || cancel())
                                .into_view(),
                        ),
                        Vec::new(),
                    )
                    .into_view()
                }
                None => app_bar("Klaxon", None, Vec::new()).into_view(),
            })
        };
        let banner = {
            let (ok, ask) = (alerts_ok, Rc::clone(&requests));
            View::show(
                move || !ok.get(),
                move || {
                    let ask = Rc::clone(&ask);
                    div()
                        .attr("id", "alerts-banner")
                        .class("klaxon-banner")
                        .child(span().text("Alarms may not show over the lock screen."))
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("id", "allow-alerts")
                                .text("Allow")
                                .on_click(move || ask(Request::AllowAlerts)),
                        )
                        .into_view()
                },
            )
        };
        let panels = {
            let (alarms, editing) = (alarms.clone(), editing.clone());
            View::dynamic(move || {
                editing.get().map_or_else(
                    || list(&alarms, &now, Rc::clone(&open), Rc::clone(&toggle)),
                    |draft| {
                        let (ask, current) = (Rc::clone(&requests), draft.sound.clone());
                        let pick: Rc<dyn Fn()> =
                            Rc::new(move || ask(Request::PickSound(current.get_untracked())));
                        editor(&draft, Rc::clone(&save), Rc::clone(&delete), &cancel, pick)
                    },
                )
            })
        };
        let fab = {
            let editing_on = editing.clone();
            View::show(
                move || editing.get().is_none(),
                move || {
                    let (alarms, editing) = (alarms.clone(), editing_on.clone());
                    button()
                        .attr("type", "button")
                        .attr("id", "add")
                        .attr("aria-label", "Add alarm")
                        .class("klaxon-fab")
                        .child(icons::plus("q-icon"))
                        .on_click(move || {
                            let id = Alarms {
                                list: alarms.get_untracked(),
                            }
                            .fresh_id();
                            editing.set(Some(Draft::new(&Alarm::new(id), true)));
                        })
                        .into_view()
                },
            )
        };
        let toast = {
            let (when, text) = (message.clone(), message);
            View::show(
                move || !when.get().is_empty(),
                move || {
                    let text = text.clone();
                    div()
                        .attr("id", "message")
                        .attr("role", "status")
                        .class("klaxon-toast")
                        .text_fn(move || text.get())
                        .into_view()
                },
            )
        };
        div()
            .attr("id", "klaxon")
            .child(bar)
            .child(div().class("klaxon-body").child(banner).child(panels))
            .child(fab)
            .child(toast)
            .into_view()
    });
    Klaxon {
        view,
        deliver,
        back,
    }
}

/// Clears `message` `ms` milliseconds after it was set, unless something else was said meanwhile.
fn dismiss_after(message: &Signal<String>, ms: u32) {
    let message = message.clone();
    let pending: Rc<std::cell::Cell<Option<quench_ui::platform::Timer>>> = Rc::default();
    create_effect(move || {
        let text = message.get();
        if let Some(timer) = pending.take() {
            timer.cancel();
        }
        if text.is_empty() {
            return;
        }
        let message = message.clone();
        pending.set(Some(quench_ui::platform::after(ms, move || {
            if message.get_untracked() == text {
                message.set(String::new());
            }
        })));
    });
}
