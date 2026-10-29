//! Daybook on Android, drawn by the native renderer with no WebView: the screens of `daybook-app`, the shared controller,
//! and the device's side done by the app's plugins (`notifications`) through the surface bridge.
//!
//! Standard output is the surface protocol's: log to standard error.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

use daybook_app::{Daybook, Setup, daybook};
use daybook_core::Request;
use daybook_host::Controller;
use daybook_host::remote::Remote;
use daybook_host::worker::{Answer, Worker};
use daybook_net::Store;
use daybook_surface::Android;
use quench_android_surface::{Bridge, run_with};
use quench_core::time::Time;
use quench_style::theme::Theme;
use quench_tz::Zone;
use quench_ui::mount;
use quench_ui_native::NativeRenderer;

/// The server the sign-in offers until another is typed.
const DEFAULT_SERVER: &str = "https://ennor.ddns.net";

/// What the worker thread brings back.
struct Done(u64, Answer);

/// How often the program looks for what a tap outside the app asked it to show.
const LAUNCH_POLL_MS: u32 = 400;

/// Shows what the Kotlin host left in the `launch` file (the widget's **+**), and looks again soon.
fn watch_launches(store: Store, deliver: Rc<dyn Fn(daybook_core::Msg)>) {
    quench_ui::platform::after(LAUNCH_POLL_MS, move || {
        if let Some(msg) = store
            .take_launch()
            .as_deref()
            .and_then(daybook_surface::launch::parse)
        {
            deliver(msg);
        }
        watch_launches(store, deliver);
    });
}

fn main() -> std::io::Result<()> {
    if std::env::args().any(|a| a == "--widget") {
        let outcome = daybook_surface::widget::refresh(&Store::standard());
        println!("{outcome:?}");
        return Ok(());
    }
    let renderer = Rc::new(NativeRenderer::new());
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
    renderer.add_stylesheet(daybook_app::STYLE);

    // The app's files are its home (the Kotlin host sets `HOME`): the session and the settings are kept there.
    let store = Store::standard();
    let settings = store.settings();
    let remote = Remote::new(store.clone());
    let has_session = remote.signed_in();

    let worker_late: Rc<RefCell<Option<Rc<Worker>>>> = Rc::default();
    let answers = {
        let worker_late = Rc::clone(&worker_late);
        renderer.channel(move |Done(id, answer): Done| {
            let worker = worker_late.borrow().clone();
            if let Some(worker) = worker {
                worker.complete(id, answer);
            }
        })
    };
    let tx = Mutex::new(answers);
    let worker = Worker::new(move |id, answer| {
        let _ = tx.lock().unwrap().send(Done(id, answer));
    });
    *worker_late.borrow_mut() = Some(Rc::clone(&worker));

    let bridge = Bridge::new();
    let platform = Rc::new(Android::new(bridge.clone(), worker, remote, store.clone()));
    let controller: Rc<RefCell<Option<Rc<Controller>>>> = Rc::default();
    let requests: Rc<dyn Fn(Request)> = {
        let controller = Rc::clone(&controller);
        Rc::new(move |request| {
            let c = controller.borrow().clone();
            if let Some(c) = c {
                c.request(request);
            }
        })
    };
    let Daybook { view, deliver } = daybook(
        requests,
        Setup {
            ask_server: true,
            server: if settings.server.is_empty() {
                DEFAULT_SERVER.to_string()
            } else {
                settings.server.clone()
            },
            list: settings.list.clone(),
            has_session,
            zone: Zone::local(),
            clock: Rc::new(Time::now),
            toast_ms: 4000,
        },
    );
    let for_launches = Rc::clone(&deliver);
    let c = Controller::new(platform, store.clone(), Rc::new(move |m| deliver(m)));
    *controller.borrow_mut() = Some(Rc::clone(&c));
    if has_session {
        c.start();
    }
    watch_launches(store, for_launches);
    let _mounted = mount(&renderer, &renderer.root(), view);
    run_with(&renderer, &bridge)
}
