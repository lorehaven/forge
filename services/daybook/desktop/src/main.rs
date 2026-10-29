//! Daybook on the desktop: a window drawn by the native renderer, the shared controller doing the work.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

use daybook_app::{Daybook, Setup, daybook};
use daybook_desktop::{DEFAULT_SERVER, Event, Later, Linux};
use daybook_host::Controller;
use daybook_host::remote::Remote;
use daybook_host::worker::Worker;
use daybook_net::Store;
use quench_core::time::Time;
use quench_style::theme::Theme;
use quench_tz::Zone;
use quench_ui::mount;
use quench_ui_native::NativeRenderer;

fn main() -> std::io::Result<()> {
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

    let store = Store::standard();
    let settings = store.settings();
    let remote = Remote::new(store.clone());
    let has_session = remote.signed_in();

    let worker_late: Rc<RefCell<Option<Rc<Worker>>>> = Rc::default();
    let events = {
        let worker_late = Rc::clone(&worker_late);
        renderer.channel(move |event: Event| match event {
            Event::Worker(id, answer) => {
                let worker = worker_late.borrow().clone();
                if let Some(worker) = worker {
                    worker.complete(id, answer);
                }
            }
        })
    };
    let tx = Mutex::new(events);
    let worker = Worker::new(move |id, answer| {
        let _ = tx.lock().unwrap().send(Event::Worker(id, answer));
    });
    *worker_late.borrow_mut() = Some(Rc::clone(&worker));
    let later: Later = Rc::new(|ms, f| {
        quench_ui::platform::after(u32::try_from(ms).unwrap_or(u32::MAX), f);
    });
    let platform = Rc::new(Linux::new(worker, remote, later));

    let controller: Rc<RefCell<Option<Rc<Controller>>>> = Rc::default();
    let requests: Rc<dyn Fn(daybook_core::Request)> = {
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
    let c = Controller::new(platform, store, Rc::new(move |m| deliver(m)));
    *controller.borrow_mut() = Some(Rc::clone(&c));
    if has_session {
        c.start();
    }
    let _mounted = mount(&renderer, &renderer.root(), view);
    quench_desktop_native::run("Daybook", "dev.lorehaven.daybook", (480, 800), &renderer)
}
