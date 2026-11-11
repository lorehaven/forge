//! Klaxon on Android, drawn by the native renderer with no `WebView`: the screens of `klaxon-app`, the alarms kept in
//! a file, and the system's side (setting the clock for the next ring) done by the app's `alarms` plugin through the
//! surface bridge.
//!
//! `--next` and `--fired <id>` are the Kotlin side's way to ask what rings next, with no window.
//!
//! Standard output is the surface protocol's: log to standard error.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use klaxon_app::live::{Msg, Request, Setup, klaxon};
use klaxon_core::{Alarms, Next};
use klaxon_surface::{Store, command, local_now};
use quench_android_surface::{Bridge, Map, Value, run_with};
use quench_style::theme::Theme;
use quench_ui::event::prevent_default;
use quench_ui::mount;
use quench_ui::prelude::{Event, EventKind, Renderer};
use quench_ui_native::NativeRenderer;

fn object(pairs: &[(&str, Value)]) -> Value {
    let mut map = Map::new();
    for (key, value) in pairs {
        map.insert(*key, value.clone());
    }
    Value::Object(map)
}

/// Tells the system when the next alarm rings (or that none does).
fn schedule(bridge: &Bridge, next: Option<Next>) {
    let args = next.map_or_else(
        || object(&[("none", Value::Bool(true))]),
        |n| {
            object(&[
                ("id", Value::from(i64::from(n.id))),
                ("days_ahead", Value::from(i64::from(n.days_ahead))),
                ("hour", Value::from(i64::from(n.hour))),
                ("minute", Value::from(i64::from(n.minute))),
                ("snooze", Value::from(i64::from(n.snooze))),
                ("vibrate", Value::Bool(n.vibrate)),
                ("loud", Value::Bool(n.loud)),
                ("sound", Value::String(n.sound)),
                ("label", Value::String(n.label)),
            ])
        },
    );
    bridge.call("alarms", "schedule", args, |_| {});
}

/// Looks at the clock and at the file again every few seconds: the minute turns, and a one-time alarm that rang while
/// the window was away has switched itself off.
fn watch(store: Store, deliver: Rc<dyn Fn(Msg)>, last: Rc<Cell<Option<std::time::SystemTime>>>) {
    quench_ui::platform::after(15_000, move || {
        deliver(Msg::Clock(local_now()));
        let stamp = store.stamp();
        if stamp != last.get() {
            last.set(stamp);
            deliver(Msg::Alarms(store.load().list));
        }
        watch(store, deliver, last);
    });
}

/// Where the app's `deliver` goes once the app exists.
type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// The framework's stylesheets, the dark and light themes, and the app's own.
fn add_styles(renderer: &NativeRenderer) {
    for (scheme, theme) in [("dark", Theme::DefaultDark), ("light", Theme::DefaultLight)] {
        let rules = Theme::theme(theme);
        renderer.add_stylesheet(
            &[
                "@media (prefers-color-scheme: ",
                scheme,
                ") {\n",
                &rules,
                "\n}",
            ]
            .concat(),
        );
    }
    for sheet in [
        quench_style::tokens::stylesheet(),
        quench_style::mobile::stylesheet(),
        quench_style::components::stylesheet(),
    ] {
        renderer.add_stylesheet(&sheet);
    }
    renderer.add_stylesheet(klaxon_app::STYLE);
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let store = Store::standard();
    if let Some(line) = command(&args, &store, local_now()) {
        println!("{line}");
        return Ok(());
    }

    let renderer = Rc::new(NativeRenderer::new());
    add_styles(&renderer);

    let bridge = Bridge::new();
    let alarms = store.load();
    // The answers of the system's sound picker come back to the app, which is made after the requests.
    let inbox: Inbox = Rc::default();
    let requests: Rc<dyn Fn(Request)> = {
        let (store, bridge, inbox) = (store.clone(), bridge.clone(), Rc::clone(&inbox));
        Rc::new(move |request| {
            let mut all: Alarms = store.load();
            match request {
                Request::Save(alarm) => all.put(alarm),
                Request::Delete(id) => all.remove(id),
                Request::Toggle(id, on) => all.set_enabled(id, on),
                Request::AllowAlerts => {
                    bridge.call("alarms", "allow", Value::Null, |_| {});
                    return;
                }
                Request::PickSound(current) => {
                    let inbox = Rc::clone(&inbox);
                    bridge.call(
                        "alarms",
                        "pick_sound",
                        object(&[("current", Value::String(current))]),
                        move |answer| {
                            // Cancelled (or refused): nothing changes.
                            let Ok(v) = answer else { return };
                            if matches!(v.get("cancelled"), Some(Value::Bool(true))) {
                                return;
                            }
                            let text = |key: &str| {
                                v.get(key)
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string()
                            };
                            let deliver = inbox.borrow().clone();
                            if let Some(deliver) = deliver {
                                deliver(Msg::Sound {
                                    uri: text("uri"),
                                    name: text("name"),
                                });
                            }
                        },
                    );
                    return;
                }
            }
            store.save(&all);
            schedule(&bridge, all.next(local_now()));
        })
    };
    let app = klaxon(
        requests,
        Setup {
            alarms: alarms.list.clone(),
            now: local_now(),
            alerts_ok: true,
            toast_ms: 3000,
        },
    );
    *inbox.borrow_mut() = Some(Rc::clone(&app.deliver));
    // The system's clock is set again whenever the program starts (an update or a force stop clears it).
    schedule(&bridge, alarms.next(local_now()));
    {
        let deliver = Rc::clone(&app.deliver);
        bridge.call("alarms", "status", Value::Null, move |answer| {
            let ok = answer.is_ok_and(|v| matches!(v.get("alerts"), Some(Value::Bool(true))));
            deliver(Msg::Alerts(ok));
        });
    }
    watch(
        store.clone(),
        Rc::clone(&app.deliver),
        Rc::new(Cell::new(store.stamp())),
    );

    // The phone's Back button reaches the program as Escape: close the editor; with nothing left, the host leaves.
    let back = Rc::clone(&app.back);
    renderer.listen(
        &renderer.root(),
        EventKind::KeyDown,
        Rc::new(move |event: &Event| {
            if matches!(event, Event::KeyDown(k) if k.key == "Escape") && back() {
                prevent_default();
            }
        }),
    );
    let _mounted = mount(&renderer, &renderer.root(), app.view);
    run_with(&renderer, &bridge)
}
