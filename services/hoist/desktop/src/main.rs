//! Hoist on the desktop: a window drawn by the native renderer, and a worker thread that talks to gatehouse and
//! warehouse. The window never waits for the network: it asks the worker through a channel and the answers come
//! back through another, handled between frames.

use std::rc::Rc;
use std::sync::mpsc;

use hoist_app::live::{Setup, hoist};
use hoist_desktop::{DEFAULT_SERVER, Store, work};
use quench_style::theme::Theme;
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
    renderer.add_stylesheet(hoist_app::STYLE);

    let store = Store::standard();
    let stored = store.read();
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
            installed: store.installed(),
            self_program: "hoist".into(),
            check_every_ms: 6 * 60 * 60 * 1000,
            auto_check: store.auto_check(),
            public: false,
        },
    );
    let deliver = Rc::clone(&app.deliver);
    let answers = renderer.channel(move |m| deliver(m));
    std::thread::spawn(move || work(&requests_rx, &answers, &store));
    let _mounted = mount(&renderer, &renderer.root(), app.view);
    quench_desktop_native::run("Hoist", "dev.lorehaven.hoist", (480, 800), &renderer)
}
