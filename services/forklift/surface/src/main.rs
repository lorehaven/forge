//! Forklift on Android, drawn by the native renderer with no WebView: the screens of `forklift-app`, the backup's
//! controller, and the device's side done by the app's plugins (`files`, `device`) through the surface bridge.
//!
//! Standard output is the surface protocol's: log to standard error.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use forklift_app::live::{Msg, Request, Setup, forklift};
use forklift_core::{DEFAULT_SERVER, Found, Manifest, Remote, Settings, storage_name};
use forklift_net::{Change, NetError, RunLock, Storage, Store, login};
use forklift_surface::worker::{
    Answer as WorkerAnswer, Shared, Worker, session_for, stored_session,
};
use forklift_surface::{Controller, Device, Node, Platform, Target, UploadEvent};
use quench_android_surface::{Bridge, CallError, Map, Value, run_with};
use quench_style::theme::Theme;
use quench_ui::mount;
use quench_ui_native::NativeRenderer;

type Then<T> = Box<dyn FnOnce(T)>;
/// Where the last page of the media library ended: kind, modified in seconds, id.
type MediaEnd = Rc<RefCell<Option<(&'static str, i64, i64)>>>;
/// Builds the controller from the manifest once that is read.
type Maker = Rc<RefCell<Option<Rc<dyn Fn(Manifest)>>>>;
/// Where the app's `deliver` goes once the app exists.
type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

fn object(pairs: &[(&str, Value)]) -> Value {
    let mut map = Map::new();
    for (key, value) in pairs {
        map.insert(*key, value.clone());
    }
    Value::Object(map)
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A MediaStore row's id: the last segment of its URI.
fn media_id(uri: &str) -> String {
    uri.rsplit('/').next().unwrap_or(uri).to_string()
}

fn found_of(row: &Value) -> Option<Found> {
    let uri = row.get("uri")?.as_str()?.to_string();
    Some(Found {
        local_id: media_id(&uri),
        name: text(row, "name"),
        size: row.get("size").and_then(Value::as_u64).unwrap_or(0),
        modified_ms: row.get("modified_ms").and_then(Value::as_i64).unwrap_or(0),
        uri,
    })
}

/// What a picked folder is called: the last part of its document id, percent-decoded (`primary:DCIM/Camera` is
/// `Camera`).
fn folder_name(uri: &str) -> String {
    let last = uri.rsplit(['/', ':']).next().unwrap_or(uri);
    let mut out = Vec::new();
    let bytes = last.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = last.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let name = String::from_utf8_lossy(&out)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    if name.is_empty() {
        "Folder".into()
    } else {
        name
    }
}

/// What the worker thread brings back.
enum Answer {
    Token(u64, WorkerAnswer),
    Login(Result<(), String>),
    /// The manifest, read on another thread.
    Manifest(Box<Manifest>),
}

struct Android {
    /// Where the last page of the media library ended: `(kind, modified in seconds, id)`. The next page is asked for
    /// by that key (MediaStore walks every row an offset skips, so reading a big library by offsets is quadratic).
    media_end: MediaEnd,
    bridge: Bridge,
    worker: Rc<Worker>,
    /// Told when the host cannot even start an upload.
    controller: Rc<RefCell<Option<Rc<Controller>>>>,
}

impl Platform for Android {
    fn media(&self, kind: &'static str, offset: u32, then: Then<Result<Vec<Found>, String>>) {
        let mut args = vec![
            ("kind", Value::String(kind.into())),
            (
                "limit",
                Value::from(i64::from(forklift_surface::MEDIA_PAGE)),
            ),
        ];
        let end = if offset == 0 {
            None
        } else {
            self.media_end.borrow().filter(|(k, _, _)| *k == kind)
        };
        match end {
            Some((_, modified, id)) => {
                args.push(("before_modified", Value::from(modified)));
                args.push(("before_id", Value::from(id)));
            }
            None => args.push(("offset", Value::from(i64::from(offset)))),
        }
        let remember = Rc::clone(&self.media_end);
        self.bridge
            .call("files", "media", object(&args), move |answer| {
                then(answer.map_err(|e: CallError| e.to_string()).map(|rows| {
                    let found: Vec<Found> = rows
                        .as_array()
                        .map(|rows| rows.iter().filter_map(found_of).collect())
                        .unwrap_or_default();
                    if let Some(last) = found.last() {
                        *remember.borrow_mut() = Some((
                            kind,
                            last.modified_ms / 1000,
                            last.local_id.parse().unwrap_or(0),
                        ));
                    }
                    found
                }));
            });
    }

    fn folder(&self, uri: &str, then: Then<Result<Vec<Node>, String>>) {
        self.bridge.call(
            "files",
            "list_folder",
            object(&[
                ("uri", Value::String(uri.into())),
                ("limit", Value::from(100_000_i64)),
            ]),
            move |answer| {
                then(answer.map_err(|e: CallError| e.to_string()).map(|rows| {
                    rows.as_array()
                        .map(|rows| {
                            rows.iter()
                                .filter_map(|row| {
                                    if matches!(row.get("is_dir"), Some(Value::Bool(true))) {
                                        Some(Node::Dir(text(row, "uri")))
                                    } else {
                                        found_of(row).map(Node::File)
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                }));
            },
        );
    }

    fn upload(&self, id: &str, uri: &str, url: &str, token: &str) {
        let headers = object(&[
            ("Authorization", Value::String(format!("Bearer {token}"))),
            (
                "Content-Type",
                Value::String("application/octet-stream".into()),
            ),
        ]);
        let (controller, id_for_error) = (Rc::clone(&self.controller), id.to_string());
        self.bridge.call(
            "files",
            "upload",
            object(&[
                ("id", Value::String(id.into())),
                ("uri", Value::String(uri.into())),
                ("url", Value::String(url.into())),
                ("method", Value::String("PUT".into())),
                ("headers", headers),
            ]),
            move |answer| {
                if let Err(e) = answer {
                    let controller = controller.borrow().clone();
                    if let Some(c) = controller {
                        c.upload_event(&UploadEvent::Failed {
                            id: id_for_error,
                            message: e.to_string(),
                        });
                    }
                }
            },
        );
    }

    fn cancel(&self, id: &str) {
        self.bridge.call(
            "files",
            "cancel_upload",
            object(&[("id", Value::String(id.into()))]),
            |_| {},
        );
    }

    fn changes(&self, storage: &str, since: i64, then: Then<Result<Vec<Change>, NetError>>) {
        self.worker.changes(storage, since, then);
    }

    fn listing(&self, storage: &str, then: Then<Result<Vec<Remote>, NetError>>) {
        self.worker.listing(storage, then);
    }

    fn quota(&self, storage: &str, then: Then<Option<(u64, u64)>>) {
        self.worker.quota(storage, then);
    }

    fn download(&self, id: &str, url: &str, token: &str, name: &str, target: &Target) {
        let (controller, id_for_error) = (Rc::clone(&self.controller), id.to_string());
        self.bridge.call(
            "restore",
            "save",
            object(&[
                ("id", Value::String(id.into())),
                ("url", Value::String(url.into())),
                ("token", Value::String(token.into())),
                ("name", Value::String(name.into())),
                ("kind", Value::String(target.kind.into())),
                ("folder", Value::String(target.folder.clone())),
            ]),
            move |answer| {
                if answer.is_err() {
                    let controller = controller.borrow().clone();
                    if let Some(c) = controller {
                        c.download_event(&id_for_error, false);
                    }
                }
            },
        );
    }

    fn storages(&self, then: Then<Result<Vec<Storage>, NetError>>) {
        self.worker.storages(then);
    }

    fn cancel_download(&self, id: &str) {
        self.bridge.call(
            "restore",
            "cancel",
            object(&[("id", Value::String(id.into()))]),
            |_| {},
        );
    }

    fn token(&self, force: bool, then: Then<Result<String, NetError>>) {
        self.worker.token(force, then);
    }

    fn device(&self, then: Then<Device>) {
        self.bridge
            .call("device", "state", Value::Null, move |answer| {
                let state = answer.unwrap_or_default();
                let flag = |key: &str| matches!(state.get(key), Some(Value::Bool(true)));
                let transport = text(&state, "transport");
                then(Device {
                    online: flag("online"),
                    wifi: transport == "wifi" || transport == "ethernet",
                    charging: flag("charging"),
                });
            });
    }

    fn pick_folder(&self, then: Then<Option<(String, String)>>) {
        self.bridge
            .call("files", "pick_folder", Value::Null, move |answer| {
                then(answer.ok().and_then(|v| {
                    let uri = v.get("uris")?.as_array()?.first()?.as_str()?.to_string();
                    Some((folder_name(&uri), uri))
                }));
            });
    }

    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }

    fn later(&self, ms: u64, f: Box<dyn FnOnce()>) {
        quench_ui::platform::after(u32::try_from(ms).unwrap_or(u32::MAX), f);
    }
}

/// The device's model, for a first storage name.
fn model() -> String {
    std::process::Command::new("getprop")
        .arg("ro.product.model")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}

/// The zone's offset from UTC in minutes, from `date +%z` (`+0200`).
fn offset_minutes() -> i32 {
    let out = std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok());
    let Some(z) = out.map(|s| s.trim().to_string()).filter(|s| s.len() == 5) else {
        return 0;
    };
    let (sign, rest) = z.split_at(1);
    let (Ok(h), Ok(m)) = (rest[..2].parse::<i32>(), rest[2..].parse::<i32>()) else {
        return 0;
    };
    (if sign == "-" { -1 } else { 1 }) * (h * 60 + m)
}

/// `--backup`: the job service's way in. A background run with no screen, speaking JSON lines (see
/// `forklift_surface::headless`).
fn background_backup() -> std::io::Result<()> {
    let store = Store::standard();
    let Some(_lock) = store.try_lock_run() else {
        println!("{{\"finished\":true,\"skipped\":\"another run is going\"}}");
        return Ok(());
    };
    forklift_surface::headless::run(std::io::stdin(), std::io::stdout(), &store).map(|_| ())
}

/// While the window is in the background the background run has the backup: this program stops sending and lets go of
/// the lock (the host marks the window hidden with a file), and takes the backup back when the window is shown again.
fn watch_background(
    store: Store,
    controller: Rc<Controller>,
    lock: Rc<RefCell<Option<RunLock>>>,
    paused: Rc<Cell<bool>>,
) {
    quench_ui::platform::after(2000, move || {
        let hidden = store.is_hidden();
        if hidden && !paused.get() {
            controller.pause_in_background();
            *lock.borrow_mut() = None;
            paused.set(true);
        } else if !hidden && paused.get() {
            // The background run may still be winding down: ask again next time if it has not let go.
            *lock.borrow_mut() = store.try_lock_run();
            if lock.borrow().is_some() {
                paused.set(false);
                controller.resume_in_foreground();
            }
        }
        watch_background(store, controller, lock, paused);
    });
}

fn main() -> std::io::Result<()> {
    if std::env::args().any(|a| a == "--backup") {
        return background_backup();
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

    // The app's files are its home (the Kotlin host sets `HOME`): the session, the settings and the manifest are kept there.
    let store = Store::standard();
    // The window has the backup to itself; a background run that is going is asked to stop by the host. The screen
    // does not wait for it: without the lock the window shows what it knows and the watcher below takes the backup
    // the moment the other lets go.
    let lock: Rc<RefCell<Option<RunLock>>> = Rc::new(RefCell::new(store.try_lock_run()));
    // Without the lock the window waits (the watcher below takes over when the other lets go).
    let paused = Rc::new(Cell::new(lock.borrow().is_none()));
    let mut settings: Settings = store.settings();
    if settings.storage.is_empty() {
        settings.storage = storage_name(&model());
        store.set_settings(&settings);
    }
    let session: Shared = Arc::new(Mutex::new(stored_session(&store, &settings.server)));
    let has_session = session.lock().unwrap().is_some();

    let bridge = Bridge::new();
    let worker_late: Rc<RefCell<Option<Rc<Worker>>>> = Rc::default();
    let controller: Rc<RefCell<Option<Rc<Controller>>>> = Rc::default();
    let app_deliver: Inbox = Rc::default();
    // Builds the controller once the manifest is read (on another thread: the first frame does not wait for it).
    let make_controller: Maker = Rc::default();

    // The worker thread's answers come back to the views' thread.
    let answers = {
        let (worker_late, controller, app_deliver, make_controller) = (
            Rc::clone(&worker_late),
            Rc::clone(&controller),
            Rc::clone(&app_deliver),
            Rc::clone(&make_controller),
        );
        renderer.channel(move |answer: Answer| match answer {
            Answer::Token(id, result) => {
                let worker = worker_late.borrow().clone();
                if let Some(worker) = worker {
                    worker.complete(id, result);
                }
            }
            Answer::Manifest(manifest) => {
                let make = make_controller.borrow().clone();
                if let Some(make) = make {
                    make(*manifest);
                }
            }
            Answer::Login(result) => {
                let deliver = app_deliver.borrow().clone();
                match result {
                    Ok(()) => {
                        if let Some(d) = deliver {
                            d(Msg::LoggedIn);
                        }
                        if let Some(c) = controller.borrow().clone() {
                            c.adopt_server();
                            c.start();
                        }
                    }
                    Err(why) => {
                        if let Some(d) = deliver {
                            d(Msg::LoginFailed(why));
                        }
                    }
                }
            }
        })
    };

    let worker = {
        let token_answers = Mutex::new(answers.clone());
        Worker::new(Arc::clone(&session), move |id, result| {
            let _ = token_answers
                .lock()
                .unwrap()
                .send(Answer::Token(id, result));
        })
    };
    *worker_late.borrow_mut() = Some(Rc::clone(&worker));
    let platform = Rc::new(Android {
        media_end: Rc::default(),
        bridge: bridge.clone(),
        worker,
        controller: Rc::clone(&controller),
    });

    let answers_for_manifest = Mutex::new(answers.clone());
    let requests: Rc<dyn Fn(Request)> = {
        let (controller, session, store, answers, app_deliver) = (
            Rc::clone(&controller),
            Arc::clone(&session),
            store.clone(),
            answers,
            Rc::clone(&app_deliver),
        );
        Rc::new(move |request| match request {
            Request::Login {
                server,
                username,
                password,
            } => {
                let (session, store, answers) =
                    (Arc::clone(&session), store.clone(), answers.clone());
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
                    let _ = answers.send(Answer::Login(result));
                });
            }
            Request::BackUpNow => {
                if let Some(c) = controller.borrow().clone() {
                    c.scan(true);
                }
            }
            Request::Settings(s) => {
                if let Some(c) = controller.borrow().clone() {
                    c.update_settings(s);
                }
            }
            Request::ListStorages => {
                if let Some(c) = controller.borrow().clone() {
                    c.list_storages();
                }
            }
            Request::ChooseStorage(name) => {
                if let Some(c) = controller.borrow().clone() {
                    c.choose_storage(&name);
                }
            }
            Request::PlanRestore => {
                if let Some(c) = controller.borrow().clone() {
                    c.plan_restore();
                }
            }
            Request::StartRestore => {
                if let Some(c) = controller.borrow().clone() {
                    c.start_restore();
                }
            }
            Request::CancelRestore => {
                if let Some(c) = controller.borrow().clone() {
                    c.cancel_restore();
                }
            }
            Request::AddFolder => {
                if let Some(c) = controller.borrow().clone() {
                    c.add_folder();
                }
            }
            Request::Logout => {
                if let Some(c) = controller.borrow().clone() {
                    c.sign_out();
                }
                *session.lock().unwrap() = None;
                if let Some(d) = app_deliver.borrow().clone() {
                    d(Msg::SignedOut);
                }
            }
        })
    };

    let app = forklift(
        requests,
        Setup {
            // The sign-in offers the usual server until another is typed (it is stored only once a sign-in works).
            settings: Settings {
                server: if settings.server.is_empty() {
                    DEFAULT_SERVER.to_string()
                } else {
                    settings.server.clone()
                },
                ..settings.clone()
            },
            has_session,
            // What the dashboard showed last time is there at the first frame.
            initial: if has_session {
                store.snapshot()
            } else {
                forklift_core::Snapshot::default()
            },
            phone: true,
            offset_minutes: offset_minutes(),
            toast_ms: 4000,
            loader_min_ms: 500,
        },
    );
    *app_deliver.borrow_mut() = Some(Rc::clone(&app.deliver));
    let deliver = Rc::clone(&app.deliver);
    // The manifest is read (it can be megabytes) on another thread, and the controller is made when it is here; the
    // screen is up long before.
    {
        let (controller, session, paused, lock, bridge, reading) = (
            Rc::clone(&controller),
            Arc::clone(&session),
            Rc::clone(&paused),
            Rc::clone(&lock),
            bridge.clone(),
            store.clone(),
        );
        let store = reading.clone();
        let make: Rc<dyn Fn(Manifest)> = Rc::new(move |manifest| {
            let deliver = Rc::clone(&deliver);
            let c = Controller::with_manifest(
                Rc::clone(&platform) as Rc<dyn Platform>,
                store.clone(),
                Rc::new(move |m| deliver(m)),
                manifest,
            );
            *controller.borrow_mut() = Some(Rc::clone(&c));
            // What the host reports of the uploads it streams.
            let (done, failed) = (Rc::clone(&c), Rc::clone(&c));
            bridge.on("files", "done", move |e| {
                let status = e
                    .get("status")
                    .and_then(Value::as_u64)
                    .and_then(|s| u16::try_from(s).ok())
                    .unwrap_or(0);
                done.upload_event(&UploadEvent::Done {
                    id: text(e, "id"),
                    status,
                });
            });
            let (saved, not_saved) = (Rc::clone(&c), Rc::clone(&c));
            bridge.on("restore", "done", move |e| {
                saved.download_event(&text(e, "id"), true);
            });
            bridge.on("restore", "failed", move |e| {
                not_saved.download_event(&text(e, "id"), false);
            });
            bridge.on("files", "failed", move |e| {
                failed.upload_event(&UploadEvent::Failed {
                    id: text(e, "id"),
                    message: text(e, "message"),
                });
            });
            if session.lock().unwrap().is_some() {
                c.show();
            }
            if session.lock().unwrap().is_some() && !paused.get() {
                c.start();
            }
            watch_background(store.clone(), c, Rc::clone(&lock), Rc::clone(&paused));
        });
        *make_controller.borrow_mut() = Some(make);
        std::thread::spawn(move || {
            let manifest = reading.manifest();
            let _ = answers_for_manifest
                .lock()
                .unwrap()
                .send(Answer::Manifest(Box::new(manifest)));
        });
    }
    let _mounted = mount(&renderer, &renderer.root(), app.view);
    run_with(&renderer, &bridge)
}
