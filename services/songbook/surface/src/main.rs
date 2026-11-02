//! Songbook on Android, drawn by the native renderer with no `WebView`. Standard output is the surface protocol's: log to
//! standard error.

use std::rc::Rc;
use std::sync::Arc;

use quench_android_surface::{Bridge, run_with};
use quench_ui::event::prevent_default;
use quench_ui::mount;
use quench_ui::prelude::{Event, EventKind, Renderer};
use quench_ui_native::NativeRenderer;
use songbook_native::Auth;
use songbook_native::store::Store;

fn main() -> std::io::Result<()> {
    let renderer = Rc::new(NativeRenderer::new());
    songbook_surface::add_styles(&renderer);

    // The app's files are its home (the Kotlin host sets `HOME`): the session is kept there, shared with the playback service.
    let store = Store::standard();
    let auth = Arc::new(Auth::new(store.clone()));
    let bridge = Bridge::new();
    let app = songbook_surface::app::build(&renderer, &bridge, auth, &store);
    // The phone's Back button reaches the program as Escape: close a box, go back a page; with nothing left, the host leaves.
    let back = app.back();
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
