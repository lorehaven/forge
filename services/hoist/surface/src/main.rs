//! Hoist on Android, drawn by the native renderer with no WebView: the same screens and the same worker as on the
//! desktop, with the install done by the app's `packages` plugin (the system's package installer) through the
//! surface bridge, and what is installed asked of the system's package manager.
//!
//! Standard output is the surface protocol's: log to standard error.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use hoist_app::live::{Msg, Request, Setup, hoist};
use hoist_app::screens::DeviceApp;
use hoist_core::{Artifact, Installed};
use hoist_desktop::{DEFAULT_SERVER, Store, work};
use quench_android_surface::{Bridge, Map, Value, run_with};
use quench_style::theme::Theme;
use quench_ui::mount;
use quench_ui_native::NativeRenderer;

/// The application id this app has on the device: what it is called in the catalog.
use hoist_core::SELF_PROGRAM;

type Deliver = Rc<dyn Fn(Msg)>;

/// The build being installed, and where its file is.
type Current = Rc<RefCell<Option<(Artifact, String)>>>;

fn object(pairs: &[(&str, Value)]) -> Value {
    let mut map = Map::new();
    for (key, value) in pairs {
        map.insert(*key, value.clone());
    }
    Value::Object(map)
}

/// Hands the downloaded file to the system's installer, once the app is allowed to install.
fn install(bridge: &Bridge, path: &str, deliver: &Deliver) {
    let (b, d, uri) = (bridge.clone(), Rc::clone(deliver), format!("file://{path}"));
    bridge.call("packages", "can_install", Value::Null, move |answer| {
        let allowed = answer.is_ok_and(|v| matches!(v.get("allowed"), Some(Value::Bool(true))));
        if !allowed {
            b.call("packages", "open_install_settings", Value::Null, |_| {});
            d(Msg::DownloadFailed(
                "Allow Hoist to install apps in the settings that opened, then try again".into(),
            ));
            return;
        }
        let failed = Rc::clone(&d);
        b.call(
            "packages",
            "install",
            object(&[("uri", Value::String(uri))]),
            move |result| {
                if let Err(e) = result {
                    failed(Msg::DownloadFailed(e.to_string()));
                }
            },
        );
    });
}

/// Asks the package manager what versions of `programs` are installed, then tells the app.
fn ask_installed(bridge: &Bridge, programs: Vec<String>, deliver: &Deliver) {
    if programs.is_empty() {
        return;
    }
    let waiting = Rc::new(RefCell::new((programs.len(), Vec::<Installed>::new())));
    for program in programs {
        let (waiting, deliver, name) = (Rc::clone(&waiting), Rc::clone(deliver), program.clone());
        bridge.call(
            "packages",
            "installed",
            object(&[("package", Value::String(program))]),
            move |answer| {
                let mut state = waiting.borrow_mut();
                if let Ok(info) = answer
                    && matches!(info.get("installed"), Some(Value::Bool(true)))
                {
                    state.1.push(Installed {
                        program: name,
                        version_code: info
                            .get("version_code")
                            .and_then(Value::as_i64)
                            .unwrap_or(0),
                        version_name: info
                            .get("version_name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    });
                }
                state.0 -= 1;
                if state.0 == 0 {
                    let known = std::mem::take(&mut state.1);
                    drop(state);
                    deliver(Msg::Known(known));
                }
            },
        );
    }
}

/// Asks the package manager for the apps of the device (the ones with a screen of their own, not the system's),
/// then tells the app.
fn ask_device(bridge: &Bridge, deliver: &Deliver) {
    let deliver = Rc::clone(deliver);
    bridge.call("packages", "list", object(&[]), move |answer| {
        let Ok(info) = answer else { return };
        let mut apps: Vec<DeviceApp> = info
            .get("apps")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter(|a| !matches!(a.get("system"), Some(Value::Bool(true))))
                    .filter_map(|a| {
                        Some(DeviceApp {
                            program: a.get("package")?.as_str()?.to_string(),
                            label: a
                                .get("label")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            version_name: a
                                .get("version_name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        apps.sort_by_cached_key(|a| (a.label.to_lowercase(), a.program.clone()));
        deliver(Msg::Device(apps));
    });
}

/// `--check`: the scheduled job's way in. Reads what the package manager says is installed from standard input (a JSON
/// list), looks for updates with the session the app keeps, and prints one line of JSON for the job to turn into a
/// notification. No window, no surface protocol.
fn background_check() -> std::io::Result<()> {
    use std::io::Read as _;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let device = hoist_desktop::check::parse_device(&input);
    let found = hoist_desktop::check::check(
        &Store::standard(),
        &device,
        std::env::consts::OS,
        std::env::consts::ARCH,
        None,
    );
    println!("{}", hoist_desktop::check::report(&found));
    Ok(())
}

fn main() -> std::io::Result<()> {
    if std::env::args().any(|a| a == "--check") {
        return background_check();
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
    renderer.add_stylesheet(hoist_app::STYLE);

    // The app's files are its home (the Kotlin host sets `HOME`): the session and the ledger are kept there.
    let store = Store::standard();
    let stored = store.read();
    let bridge = Bridge::new();
    let current: Current = Rc::default();
    let (worker_tx, worker_rx) = mpsc::channel::<Request>();

    // An install is a download here; the system's installer does the rest, and its answer ends it.
    let requests: Rc<dyn Fn(Request)> = {
        let (worker_tx, current, bridge) = (worker_tx, Rc::clone(&current), bridge.clone());
        Rc::new(move |request| {
            let request = match request {
                Request::Launch(program) => {
                    bridge.call(
                        "packages",
                        "launch",
                        object(&[("package", Value::String(program))]),
                        |_| {},
                    );
                    return;
                }
                Request::InstallPublic(app) => {
                    // The install event names what was installed: the app, as the ledger knows builds.
                    *current.borrow_mut() = Some((
                        Artifact {
                            program: app.package_name.clone(),
                            platform: "android".into(),
                            arch: None,
                            format: "apk".into(),
                            version_code: app.version_code,
                            version_name: app.version_name.clone(),
                            filename: format!("{}-{}.apk", app.package_name, app.version_code),
                            size_bytes: app.size_bytes,
                            sha256: app.sha256.clone().unwrap_or_default(),
                            label: Some(app.title().to_string()),
                            min_sdk_version: None,
                            target_sdk_version: None,
                            permissions: Vec::new(),
                            uploaded_by: "public".into(),
                            yanked: false,
                            created_at: String::new(),
                        },
                        String::new(),
                    ));
                    Request::DownloadPublic(app)
                }
                Request::Install(artifact) => {
                    *current.borrow_mut() = Some((artifact.clone(), String::new()));
                    Request::Download(artifact)
                }
                other => other,
            };
            let _ = worker_tx.send(request);
        })
    };
    let app = hoist(
        requests,
        Setup {
            server: stored
                .as_ref()
                .map_or_else(|| DEFAULT_SERVER.to_string(), |s| s.0.clone()),
            has_session: stored.is_some(),
            installed: store.installed(),
            self_program: SELF_PROGRAM.into(),
            check_every_ms: 6 * 60 * 60 * 1000,
            auto_check: store.auto_check(),
            public: true,
            toast_ms: 4000,
            loader_min_ms: 500,
        },
    );
    let deliver: Deliver = Rc::clone(&app.deliver);

    // What the system's installer reports.
    {
        let (deliver, current, store, again) = (
            Rc::clone(&deliver),
            Rc::clone(&current),
            store.clone(),
            bridge.clone(),
        );
        bridge.on("packages", "install", move |event| {
            let status = event
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match status {
                "success" => {
                    if let Some((artifact, path)) = current.borrow_mut().take() {
                        let installed = Installed {
                            program: artifact.program.clone(),
                            version_code: artifact.version_code,
                            version_name: artifact.version_name.clone(),
                        };
                        store.record(&installed);
                        deliver(Msg::Installed {
                            program: installed.program,
                            version_code: installed.version_code,
                            version_name: installed.version_name,
                            path,
                        });
                        // What is on the device changed.
                        ask_device(&again, &deliver);
                    }
                }
                "failure" | "cancelled" => {
                    current.borrow_mut().take();
                    let why = event
                        .get("message")
                        .and_then(Value::as_str)
                        .filter(|m| !m.is_empty());
                    deliver(Msg::DownloadFailed(why.map_or_else(
                        || "the install was cancelled".into(),
                        String::from,
                    )));
                }
                _ => {}
            }
        });
    }

    // What the worker says comes to the views; a finished download goes to the installer, and a catalog makes
    // the app ask the system what of it is installed.
    let answers = {
        let (deliver, bridge, current) = (Rc::clone(&deliver), bridge.clone(), Rc::clone(&current));
        renderer.channel(move |message: Msg| match message {
            Msg::Downloaded { program, path } => {
                if let Some((_, file)) = current.borrow_mut().as_mut() {
                    file.clone_from(&path);
                }
                install(&bridge, &path, &deliver);
                let _ = program;
            }
            Msg::Catalog(rows) => {
                let mut programs: Vec<String> = rows.iter().map(|a| a.program.clone()).collect();
                programs.sort();
                programs.dedup();
                deliver(Msg::Catalog(rows));
                ask_installed(&bridge, programs, &deliver);
                ask_device(&bridge, &deliver);
            }
            other => deliver(other),
        })
    };
    std::thread::spawn(move || work(&worker_rx, &answers, &store));
    let _mounted = mount(&renderer, &renderer.root(), app.view);
    run_with(&renderer, &bridge)
}
