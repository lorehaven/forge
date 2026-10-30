//! Forklift on the desktop: a window drawn by the native renderer, the folders the person chose backed up by the shared
//! controller, and `forklift --backup` for a timer.

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use forklift_app::live::{Forklift, Msg, Request, Setup, forklift};
use forklift_core::{DEFAULT_SERVER, Settings};
use forklift_desktop::{
    Event, Linux, Post, default_storage, notify_desktop, offset_minutes, run_background,
};
use forklift_host::Controller;
use forklift_host::worker::{Shared, Worker, session_for, stored_session};
use forklift_net::{RunLock, Store, login};
use quench_style::theme::Theme;
use quench_ui::mount;
use quench_ui_native::NativeRenderer;

/// `forklift --backup`: one backup with no window, for a systemd timer. Says what it did, and with a desktop
/// notification when it sent something.
fn background() -> ExitCode {
    let store = Store::standard();
    let Some(_lock) = store.try_lock_run() else {
        println!("the backup is running already (the window has it)");
        return ExitCode::SUCCESS;
    };
    match run_background(&store) {
        Ok(ran) => {
            if let Some(why) = ran.skipped {
                println!("nothing done: {why}");
            } else {
                println!("{} file(s) sent", ran.sent);
                if ran.sent > 0 {
                    notify_desktop(
                        "Forklift",
                        &format!(
                            "Backed up {} new file{}",
                            ran.sent,
                            if ran.sent == 1 { "" } else { "s" }
                        ),
                    );
                }
            }
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("the backup could not run: {why}");
            ExitCode::FAILURE
        }
    }
}

/// Where the app's `deliver` goes once the app exists.
type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

fn main() -> std::io::Result<()> {
    if std::env::args().any(|a| a == "--backup") {
        let code = background();
        std::process::exit(if code == ExitCode::SUCCESS { 0 } else { 1 });
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
    renderer.add_stylesheet(forklift_app::STYLE);

    let store = Store::standard();
    // The window has the backup to itself; a timer's run that is going is waited for a moment, no longer.
    let _lock: Option<RunLock> = (0..20).find_map(|_| {
        let lock = store.try_lock_run();
        if lock.is_none() {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        lock
    });
    // A computer backs up folders: no media library to look through.
    let mut settings = store.settings();
    let first = Settings {
        storage: default_storage(),
        photos: false,
        videos: false,
        ..settings.clone()
    };
    if settings.storage.is_empty() || settings.photos || settings.videos {
        settings = first;
        store.set_settings(&settings);
    }
    let session: Shared = Arc::new(Mutex::new(stored_session(&store, &settings.server)));
    let has_session = session.lock().unwrap().is_some();

    let controller: Rc<RefCell<Option<Rc<Controller>>>> = Rc::default();
    let worker_late: Rc<RefCell<Option<Rc<Worker>>>> = Rc::default();
    let app_deliver: Inbox = Rc::default();

    // What the other threads say comes back here, between frames.
    let events = {
        let (controller, worker_late, app_deliver) = (
            Rc::clone(&controller),
            Rc::clone(&worker_late),
            Rc::clone(&app_deliver),
        );
        renderer.channel(move |event: Event| {
            let (controller, deliver) = (controller.borrow().clone(), app_deliver.borrow().clone());
            match event {
                Event::Worker(id, answer) => {
                    let worker = worker_late.borrow().clone();
                    if let Some(worker) = worker {
                        worker.complete(id, answer);
                    }
                }
                Event::Upload(e) => {
                    if let Some(c) = controller {
                        c.upload_event(&e);
                    }
                }
                Event::Download { id, ok } => {
                    if let Some(c) = controller {
                        c.download_event(&id, ok);
                    }
                }
                Event::Notice(text) => {
                    if let Some(d) = deliver {
                        d(Msg::Notice(text));
                    }
                }
                Event::Login(result) => match result {
                    Ok(()) => {
                        if let Some(d) = deliver {
                            d(Msg::LoggedIn);
                        }
                        if let Some(c) = controller {
                            c.adopt_server();
                            c.start();
                        }
                    }
                    Err(why) => {
                        if let Some(d) = deliver {
                            d(Msg::LoginFailed(why));
                        }
                    }
                },
            }
        })
    };
    let (worker_tx, post_tx) = (Mutex::new(events.clone()), Mutex::new(events.clone()));
    let worker = Worker::new(Arc::clone(&session), move |id, answer| {
        let _ = worker_tx.lock().unwrap().send(Event::Worker(id, answer));
    });
    *worker_late.borrow_mut() = Some(Rc::clone(&worker));
    let post: Post = Arc::new(move |event| {
        let _ = post_tx.lock().unwrap().send(event);
    });
    let later = Box::new(|ms: u64, f: Box<dyn FnOnce()>| {
        quench_ui::platform::after(u32::try_from(ms).unwrap_or(u32::MAX), f);
    });
    let platform = Rc::new(Linux::new(worker, Arc::clone(&post), store.clone(), later));

    let requests: Rc<dyn Fn(Request)> = {
        let (controller, session, store, post, app_deliver) = (
            Rc::clone(&controller),
            Arc::clone(&session),
            store.clone(),
            Arc::clone(&post),
            Rc::clone(&app_deliver),
        );
        Rc::new(move |request| {
            let c = controller.borrow().clone();
            match request {
                Request::Login {
                    server,
                    username,
                    password,
                } => {
                    let (session, store, post) =
                        (Arc::clone(&session), store.clone(), Arc::clone(&post));
                    std::thread::spawn(move || {
                        let result = login(&server, &username, &password)
                            .map(|s| {
                                store.set_token(s.refresh_token());
                                let mut settings = store.settings();
                                settings.server.clone_from(&server);
                                store.set_settings(&settings);
                                *session.lock().unwrap() =
                                    Some(session_for(&store, &server, s.refresh_token()));
                            })
                            .map_err(|e| e.to_string());
                        post(Event::Login(result));
                    });
                }
                Request::BackUpNow => {
                    if let Some(c) = c {
                        c.scan(true);
                    }
                }
                Request::Settings(s) => {
                    if let Some(c) = c {
                        c.update_settings(s);
                    }
                }
                Request::ListStorages => {
                    if let Some(c) = c {
                        c.list_storages();
                    }
                }
                Request::ChooseStorage(name) => {
                    if let Some(c) = c {
                        c.choose_storage(&name);
                    }
                }
                Request::AddFolder => {
                    if let Some(c) = c {
                        c.add_folder();
                    }
                }
                Request::PlanRestore => {
                    if let Some(c) = c {
                        c.plan_restore();
                    }
                }
                Request::StartRestore => {
                    if let Some(c) = c {
                        c.start_restore();
                    }
                }
                Request::CancelRestore => {
                    if let Some(c) = c {
                        c.cancel_restore();
                    }
                }
                Request::Logout => {
                    if let Some(c) = c {
                        c.sign_out();
                    }
                    *session.lock().unwrap() = None;
                    let deliver = app_deliver.borrow().clone();
                    if let Some(d) = deliver {
                        d(Msg::SignedOut);
                    }
                }
            }
        })
    };

    let store_for_snapshot = store.clone();
    let Forklift { view, deliver } = forklift(
        requests,
        Setup {
            // The sign-in offers the usual server until another is typed.
            settings: Settings {
                server: if settings.server.is_empty() {
                    DEFAULT_SERVER.to_string()
                } else {
                    settings.server.clone()
                },
                ..settings.clone()
            },
            has_session,
            initial: if has_session {
                store_for_snapshot.snapshot()
            } else {
                forklift_core::Snapshot::default()
            },
            phone: false,
            offset_minutes: offset_minutes(),
            toast_ms: 4000,
            loader_min_ms: 500,
        },
    );
    *app_deliver.borrow_mut() = Some(Rc::clone(&deliver));
    let emit = Rc::clone(&deliver);
    let c = Controller::new(platform, store, Rc::new(move |m| emit(m)));
    *controller.borrow_mut() = Some(Rc::clone(&c));
    if has_session {
        c.start();
    }
    let _mounted = mount(&renderer, &renderer.root(), view);
    quench_desktop_native::run("Forklift", "dev.lorehaven.forklift", (480, 800), &renderer)
}
