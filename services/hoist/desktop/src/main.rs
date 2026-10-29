//! Hoist on the desktop: a window drawn by the native renderer, and a worker thread that talks to gatehouse and
//! warehouse. The window never waits for the network: it asks the worker through a channel and the answers come
//! back through another, handled between frames.

use std::rc::Rc;
use std::sync::mpsc;

use hoist_app::live::{Setup, hoist};
use hoist_core::SELF_PROGRAM;
use hoist_desktop::check::{Outcome, check, words};
use hoist_desktop::{DEFAULT_SERVER, Store, notify_desktop, this_app, work};
use quench_style::theme::Theme;
use quench_ui::mount;
use quench_ui_native::NativeRenderer;

/// `hoist --check`: looks for updates without a window and, when there is news, says so with a desktop notification.
/// A systemd user timer (or cron) can run it now and then; the "check for updates in the background" setting turns
/// it off. Prints what it found.
fn background_check() -> std::process::ExitCode {
    let store = Store::standard();
    let formats = store.installable_formats();
    let found = check(
        &store,
        &[this_app()],
        "linux",
        std::env::consts::ARCH,
        Some(&formats),
    );
    match found {
        Ok(Outcome::Off) => println!("the background check is switched off in the settings"),
        Ok(Outcome::SignedOut) => println!("not signed in: open Hoist and sign in"),
        Ok(Outcome::Updates { updates, notify }) => {
            let (title, body) = words(&updates);
            if updates.is_empty() {
                println!("no updates");
            } else {
                println!("{title}: {body}");
            }
            if notify {
                notify_desktop(&format!("Hoist: {title}"), &body);
            }
        }
        Err(why) => {
            eprintln!("could not check: {why}");
            return std::process::ExitCode::FAILURE;
        }
    }
    std::process::ExitCode::SUCCESS
}

fn main() -> std::io::Result<()> {
    if std::env::args().any(|a| a == "--check") {
        let done = background_check() == std::process::ExitCode::SUCCESS;
        std::process::exit(i32::from(!done));
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

    let store = Store::standard();
    let stored = store.read();
    // This running program is installed, at the version it is: a package manager may have put it here, which the
    // ledger of what this app installed knows nothing of.
    let mut installed = store.installed();
    installed.insert(SELF_PROGRAM.to_string(), this_app());
    let (requests_tx, requests_rx) = mpsc::channel();
    let app = hoist(
        Rc::new(move |r| {
            let _ = requests_tx.send(r);
        }),
        Setup {
            server: stored
                .as_ref()
                .map_or_else(|| DEFAULT_SERVER.to_string(), |s| s.0.clone()),
            has_session: stored.is_some(),
            installed,
            self_program: SELF_PROGRAM.into(),
            check_every_ms: 6 * 60 * 60 * 1000,
            auto_check: store.auto_check(),
            public: false,
            toast_ms: 4000,
            loader_min_ms: 500,
        },
    );
    let deliver = Rc::clone(&app.deliver);
    let answers = renderer.channel(move |m| deliver(m));
    std::thread::spawn(move || work(&requests_rx, &answers, &store));
    let _mounted = mount(&renderer, &renderer.root(), app.view);
    quench_desktop_native::run("Hoist", "dev.lorehaven.hoist", (480, 800), &renderer)
}
